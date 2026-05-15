#!/usr/bin/env python3
from __future__ import annotations

import argparse
import atexit
import json
import os
import platform
import statistics
import sys
import tempfile
import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
PYTHON_SRC = REPO_ROOT / "python" / "src"
DEFAULT_BUDGETS = REPO_ROOT / "benchmarks" / "perf_budgets.json"
DEFAULT_RESULTS = REPO_ROOT / "target" / "benchmark-results.json"

if str(PYTHON_SRC) not in sys.path:
    sys.path.insert(0, str(PYTHON_SRC))

from peekaboox.agent import AgentRuntime  # noqa: E402
from peekaboox.client import (  # noqa: E402
    ActionResult,
    CaptureDeltaResult,
    CaptureMetadata,
    DesktopState,
    Rect,
    UiElement,
    WindowInfo,
)
from peekaboox.mcp import McpServer  # noqa: E402
from peekaboox.memory import MemoryStore  # noqa: E402
from peekaboox.plugins import PLUGIN_MANIFEST_FILE, PLUGIN_SDK_VERSION  # noqa: E402
from peekaboox.security import JsonlAuditLogger  # noqa: E402
from peekaboox.workflows import dump_workflow_text, load_workflow_text  # noqa: E402


Runner = Callable[[], None]


@dataclass(frozen=True, slots=True)
class BenchmarkCase:
    name: str
    runner: Runner


@dataclass(frozen=True, slots=True)
class CaseBudget:
    description: str
    target_ms: float
    iterations: int
    warmup: int


class BenchmarkClient:
    def __init__(self, state: DesktopState) -> None:
        self.state = state
        self.clicks = 0

    def click(
        self,
        x: int | None = None,
        y: int | None = None,
        semantic_selector: str | None = None,
        vision_fallback: bool = False,
    ) -> ActionResult:
        if semantic_selector is not None and not semantic_selector.strip():
            raise ValueError("semantic_selector must not be empty")
        if semantic_selector is None and (x is None or y is None):
            raise ValueError("coordinates or semantic_selector are required")
        self.clicks += 1
        return ActionResult(ok=True, message="clicked")

    def click_selector(self, selector: str, vision_fallback: bool = False) -> ActionResult:
        return self.click(semantic_selector=selector, vision_fallback=vision_fallback)

    def get_desktop_state(self) -> DesktopState:
        return self.state

    def list_windows(self) -> tuple[WindowInfo, ...]:
        return self.state.windows

    def find_element(
        self,
        selector: str,
        vision_fallback: bool = False,
    ) -> tuple[UiElement, ...]:
        return tuple(
            element
            for element in self.state.elements
            if selector.casefold() in f"{element.role} {element.label or ''}".casefold()
        )

    def capture_delta(
        self,
        stream_id: str = "default",
        reset: bool = False,
        region: Rect | None = None,
        window_id: str | None = None,
        per_channel_threshold: int | None = None,
        low_bandwidth: bool = True,
    ) -> CaptureDeltaResult:
        if region is not None and window_id is not None:
            raise ValueError("provide either region or window_id, not both")
        bounds = region or Rect(x=8, y=12, width=64, height=16)
        return CaptureDeltaResult(
            stream_id=stream_id,
            sequence=1 if reset else 2,
            low_bandwidth=low_bandwidth,
            full_frame=reset,
            frame_width=1280,
            frame_height=720,
            pixel_format="rgba8",
            capture_region=region,
            changed_bounds=bounds,
            changed_pixels=bounds.width * bounds.height,
            changed_ratio=(bounds.width * bounds.height) / (1280 * 720),
            patch_stride=bounds.width * 4,
            patch=b"\x00" * (bounds.width * bounds.height * 4),
            metadata=CaptureMetadata(
                width=1280,
                height=720,
                backend="benchmark/direct",
                captured_at_unix_ms=1,
            ),
        )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run deterministic PeekabooX performance regression benchmarks."
    )
    parser.add_argument(
        "--budgets",
        default=str(DEFAULT_BUDGETS),
        help="JSON file with benchmark budgets",
    )
    parser.add_argument(
        "--json-out",
        default=str(DEFAULT_RESULTS),
        help="write machine-readable benchmark results to this path",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        help="override iteration count for every selected benchmark",
    )
    parser.add_argument(
        "--case",
        action="append",
        dest="cases",
        help="run only the named benchmark case; may be repeated",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="list benchmark cases and exit",
    )
    parser.add_argument(
        "--no-fail",
        action="store_true",
        help="do not exit non-zero when a budget is exceeded",
    )
    args = parser.parse_args()

    cases = benchmark_cases()
    budgets = load_budgets(Path(args.budgets))
    selected = select_cases(cases, args.cases)

    if args.list:
        for case in selected:
            budget = budgets[case.name]
            print(f"{case.name}: target p95 <= {budget.target_ms:.3f}ms")
        return 0

    results = [
        run_case(
            case,
            budgets[case.name],
            iterations_override=args.iterations,
        )
        for case in selected
    ]
    failed = [result for result in results if not result["passed"]]
    payload = {
        "schema_version": 1,
        "generated_at_unix_ms": int(time.time() * 1000),
        "environment": {
            "python": platform.python_version(),
            "platform": platform.platform(),
            "processor": platform.processor(),
            "pid": os.getpid(),
        },
        "budgets": str(Path(args.budgets)),
        "results": results,
        "failed": [result["name"] for result in failed],
    }

    output_path = Path(args.json_out)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print_summary(results, output_path)

    if failed and not args.no_fail:
        return 1
    return 0


def benchmark_cases() -> dict[str, BenchmarkCase]:
    state = sample_desktop_state(element_count=240)
    memory = MemoryStore()
    memory.ingest_desktop_state(state, snapshot_id="snapshot:benchmark", captured_at_unix_ms=1)
    runtime = AgentRuntime(client=BenchmarkClient(state), memory=memory)
    server = McpServer(runtime=runtime)
    server.register_default_tools()
    plugin_root = REPO_ROOT / "target" / "benchmark-plugins"
    plugin_dir = plugin_root / "demo"
    plugin_dir.mkdir(parents=True, exist_ok=True)
    (plugin_dir / PLUGIN_MANIFEST_FILE).write_text(
        json.dumps(
            {
                "schema_version": PLUGIN_SDK_VERSION,
                "id": "benchmark.demo",
                "name": "Benchmark Demo",
                "version": "1.0.0",
                "tools": [{"name": "benchmark.inspect", "description": "Inspect benchmark state"}],
            }
        ),
        encoding="utf-8",
    )
    workflow_text = (REPO_ROOT / "examples" / "workflow.yaml").read_text(encoding="utf-8")
    audit_path = Path(tempfile.gettempdir()) / f"peekaboox-benchmark-audit-{os.getpid()}.jsonl"
    cleanup_path(audit_path)
    atexit.register(cleanup_path, audit_path)
    audit_logger = JsonlAuditLogger(audit_path, source="benchmark")

    def workflow_yaml_roundtrip() -> None:
        workflow = load_workflow_text(workflow_text, format_name="yaml")
        dumped = dump_workflow_text(workflow, format_name="json")
        if "steps" not in dumped:
            raise AssertionError("workflow JSON did not contain steps")

    def desktop_graph_ingest() -> None:
        store = MemoryStore()
        snapshot = store.ingest_desktop_state(state, snapshot_id="snapshot:iteration")
        if len(snapshot.nodes) < len(state.elements):
            raise AssertionError("desktop graph ingest dropped element nodes")

    def cached_selector_query() -> None:
        elements = memory.find_cached_elements("role=push button,label=Submit 42")
        if len(elements) != 1:
            raise AssertionError(f"expected exactly one cached match, got {len(elements)}")

    def cached_click_cycle() -> None:
        result = runtime.click_selector("role=push button,label=Submit 42")
        if not result.ok:
            raise AssertionError("cached click cycle failed")

    def capture_delta_cycle() -> None:
        delta = runtime.capture_delta(
            stream_id="benchmark",
            region=Rect(x=8, y=12, width=64, height=16),
            per_channel_threshold=2,
            low_bandwidth=True,
        )
        if delta.patch_stride == 0 or not delta.patch:
            raise AssertionError("capture delta cycle returned an empty changed patch")

    def capture_delta_full_frame_cycle() -> None:
        delta = runtime.capture_delta(
            stream_id="benchmark-full-frame",
            region=Rect(x=8, y=12, width=64, height=16),
            per_channel_threshold=2,
            low_bandwidth=False,
        )
        if delta.low_bandwidth:
            raise AssertionError("full-frame capture delta cycle did not preserve mode")

    def mcp_list_tools() -> None:
        payload = json.dumps(server.list_tools(), sort_keys=True)
        if "capture_screen" not in payload:
            raise AssertionError("MCP tool list did not contain capture_screen")

    def plugin_discovery() -> None:
        result = runtime.list_plugins(paths=(plugin_root,))
        if not result.plugins or result.plugins[0].manifest.id != "benchmark.demo":
            raise AssertionError("plugin discovery did not return the benchmark plugin")

    def audit_jsonl_write() -> None:
        audit_logger.write(
            "benchmark",
            "ok",
            {"case": "python.audit.jsonl_write", "pid": os.getpid()},
        )

    return {
        "python.workflow_yaml_roundtrip": BenchmarkCase(
            "python.workflow_yaml_roundtrip",
            workflow_yaml_roundtrip,
        ),
        "python.memory.desktop_graph_ingest": BenchmarkCase(
            "python.memory.desktop_graph_ingest",
            desktop_graph_ingest,
        ),
        "python.memory.cached_selector_query": BenchmarkCase(
            "python.memory.cached_selector_query",
            cached_selector_query,
        ),
        "python.agent.cached_click_cycle": BenchmarkCase(
            "python.agent.cached_click_cycle",
            cached_click_cycle,
        ),
        "python.agent.capture_delta_cycle": BenchmarkCase(
            "python.agent.capture_delta_cycle",
            capture_delta_cycle,
        ),
        "python.agent.capture_delta_full_frame_cycle": BenchmarkCase(
            "python.agent.capture_delta_full_frame_cycle",
            capture_delta_full_frame_cycle,
        ),
        "python.mcp.list_tools": BenchmarkCase(
            "python.mcp.list_tools",
            mcp_list_tools,
        ),
        "python.plugins.discovery": BenchmarkCase(
            "python.plugins.discovery",
            plugin_discovery,
        ),
        "python.audit.jsonl_write": BenchmarkCase(
            "python.audit.jsonl_write",
            audit_jsonl_write,
        ),
    }


def sample_desktop_state(element_count: int) -> DesktopState:
    window = WindowInfo(
        id="window-1",
        title="Benchmark App",
        app_id="org.peekaboox.Benchmark",
        bounds=Rect(x=0, y=0, width=1280, height=720),
        focused=True,
        state="normal",
    )
    elements = tuple(
        UiElement(
            id=f"button-{index}",
            role="push button" if index % 3 == 0 else "text",
            label=f"Submit {index}" if index % 3 == 0 else f"Label {index}",
            bounds=Rect(
                x=10 + (index % 12) * 90,
                y=20 + (index // 12) * 32,
                width=72 if index % 3 == 0 else 48,
                height=24,
            ),
            confidence=0.95,
            states=("enabled", "visible"),
        )
        for index in range(element_count)
    )
    return DesktopState(active_window=window, windows=(window,), elements=elements)


def load_budgets(path: Path) -> dict[str, CaseBudget]:
    raw = json.loads(path.read_text(encoding="utf-8"))
    raw_budgets = raw.get("budgets")
    if not isinstance(raw_budgets, dict):
        raise ValueError("benchmark budgets file must contain a budgets object")

    budgets: dict[str, CaseBudget] = {}
    for name, value in raw_budgets.items():
        if not isinstance(value, dict):
            raise ValueError(f"benchmark budget for {name} must be an object")
        budgets[name] = CaseBudget(
            description=str(value.get("description", "")),
            target_ms=float(value["target_ms"]),
            iterations=int(value.get("iterations", 100)),
            warmup=int(value.get("warmup", 10)),
        )
    return budgets


def select_cases(
    cases: dict[str, BenchmarkCase],
    requested: list[str] | None,
) -> list[BenchmarkCase]:
    if not requested:
        return list(cases.values())
    selected = []
    for name in requested:
        if name not in cases:
            known = ", ".join(cases)
            raise ValueError(f"unknown benchmark case {name!r}; expected one of: {known}")
        selected.append(cases[name])
    return selected


def run_case(
    case: BenchmarkCase,
    budget: CaseBudget,
    *,
    iterations_override: int | None,
) -> dict[str, Any]:
    iterations = iterations_override or budget.iterations
    if iterations <= 0:
        raise ValueError("iterations must be positive")

    for _ in range(max(0, budget.warmup)):
        case.runner()

    samples = []
    for _ in range(iterations):
        started = time.perf_counter_ns()
        case.runner()
        elapsed_ns = time.perf_counter_ns() - started
        samples.append(elapsed_ns / 1_000_000)

    p95_ms = percentile(samples, 95.0)
    return {
        "name": case.name,
        "description": budget.description,
        "iterations": iterations,
        "warmup": budget.warmup,
        "target_ms": budget.target_ms,
        "min_ms": min(samples),
        "median_ms": statistics.median(samples),
        "mean_ms": statistics.fmean(samples),
        "p95_ms": p95_ms,
        "max_ms": max(samples),
        "passed": p95_ms <= budget.target_ms,
    }


def percentile(samples: list[float], value: float) -> float:
    ordered = sorted(samples)
    if not ordered:
        raise ValueError("cannot compute percentile for empty samples")
    index = round((len(ordered) - 1) * (value / 100.0))
    return ordered[index]


def print_summary(results: list[dict[str, Any]], output_path: Path) -> None:
    for result in results:
        status = "ok" if result["passed"] else "regression"
        print(
            (
                "{status:10} {name:42} "
                "p95={p95:.3f}ms target={target:.3f}ms median={median:.3f}ms"
            ).format(
                status=status,
                name=result["name"],
                p95=result["p95_ms"],
                target=result["target_ms"],
                median=result["median_ms"],
            )
        )
    print(f"wrote {output_path}")


def cleanup_path(path: Path) -> None:
    try:
        path.unlink()
    except FileNotFoundError:
        pass


if __name__ == "__main__":
    raise SystemExit(main())
