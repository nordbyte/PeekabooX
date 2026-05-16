import argparse
import json
import os
import sys
import time
from collections.abc import Callable
from dataclasses import dataclass, field, fields, is_dataclass, replace
from pathlib import Path
from typing import Any, Sequence

from peekaboox.client import (
    ActionResult,
    CaptureBackendsResult,
    CaptureDeltaResult,
    CaptureScreenResult,
    DetectUiElementsResult,
    DesktopActionResult,
    DesktopLocateResult,
    DesktopState,
    DmaBufProbeResult,
    OcrResult,
    PeekabooXClient,
    Rect,
    UiElement,
    UiStateResult,
    VisualDiffResult,
    WindowListResult,
    WindowInfo,
)
from peekaboox.client import DEFAULT_GRPC_TARGET
from peekaboox.doctor import DoctorResult, run_doctor
from peekaboox.memory import (
    DesktopGraphSnapshot,
    DesktopGraphStatus,
    DesktopGraphUpdate,
    GraphEdge,
    GraphNode,
    MemoryStore,
    SQLiteMemoryStore,
)
from peekaboox.planning import PlanningEngine
from peekaboox.plugins import (
    PluginToolExecutionResult,
    discover_plugins,
    execute_plugin_tool,
    PluginDiscoveryResult,
)
from peekaboox.security import (
    Capability,
    CapabilityAuditEvent,
    CapabilityPolicy,
    ConfirmationAuditEvent,
    ConfirmationPolicy,
    DangerousAction,
    JsonlAuditLogger,
    KNOWN_CAPABILITY_PROFILES,
)
from peekaboox.workflows import (
    Workflow,
    WorkflowRecorder,
    WorkflowStep,
    load_workflow_file,
    save_workflow_file,
)
from peekaboox import __version__ as PEEKABOOX_VERSION


WINDOW_SORT_CHOICES = ("backend", "focused", "title", "app", "area", "id", "state")
WINDOW_BACKEND_CHOICES = ("auto", "gnome", "at-spi", "xdotool")


@dataclass(frozen=True, slots=True)
class VerificationResult:
    ok: bool
    message: str
    metadata: dict[str, object] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class ActionAttempt:
    attempt: int
    ok: bool
    message: str
    result: object | None = None
    error: str | None = None
    verification: VerificationResult | None = None
    recovery: dict[str, object] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class StepExecutionResult:
    step: WorkflowStep
    ok: bool
    attempts: tuple[ActionAttempt, ...]
    result: object | None
    recovery: dict[str, object]


@dataclass(frozen=True, slots=True)
class WorkflowExecutionResult:
    goal: str
    ok: bool
    steps: tuple[StepExecutionResult, ...]
    recovery: dict[str, object]


@dataclass(frozen=True, slots=True)
class PreflightResult:
    operation: str
    required_categories: tuple[str, ...]
    ok: bool
    blocked_categories: tuple[str, ...]
    warning_categories: tuple[str, ...]
    category_status: dict[str, str] = field(default_factory=dict)
    category_severity: dict[str, str] = field(default_factory=dict)
    messages: tuple[str, ...] = ()
    doctor_status: str = "ok"


@dataclass(frozen=True, slots=True)
class PreflightAuditEvent:
    operation: str
    status: str
    mode: str
    ok: bool
    occurred_at_unix_ms: int
    required_categories: tuple[str, ...]
    blocked_categories: tuple[str, ...]
    warning_categories: tuple[str, ...]
    metadata: dict[str, object] = field(default_factory=dict)


class PreflightError(RuntimeError):
    def __init__(self, result: PreflightResult) -> None:
        super().__init__(_preflight_error_message(result))
        self.result = result


Verifier = Callable[[WorkflowStep, object], VerificationResult | bool]


@dataclass(slots=True)
class AgentRuntime:
    """Small orchestration shell for daemon RPCs, workflows, and memory."""

    retries: int = 2
    tools: dict[str, object] = field(default_factory=dict)
    client: PeekabooXClient | None = None
    planner: PlanningEngine = field(default_factory=PlanningEngine)
    memory: MemoryStore = field(default_factory=MemoryStore)
    recorder: WorkflowRecorder | None = None
    last_recording: Workflow | None = None
    capability_policy: CapabilityPolicy = field(default_factory=CapabilityPolicy.allow_all)
    confirmation_policy: ConfirmationPolicy = field(default_factory=ConfirmationPolicy.disabled)
    audit_logger: JsonlAuditLogger | None = None
    plugin_paths: tuple[Path, ...] = ()
    plugin_registry: PluginDiscoveryResult | None = None
    preflight_mode: str | None = None
    preflight_timeout_seconds: float = 30.0
    preflight_audit_events: list[PreflightAuditEvent] = field(default_factory=list)
    _preflight_doctor_result: DoctorResult | None = field(default=None, init=False, repr=False)

    def __post_init__(self) -> None:
        self.preflight_mode = _normalize_preflight_mode(self.preflight_mode)
        if self.preflight_timeout_seconds <= 0:
            raise ValueError("preflight_timeout_seconds must be greater than zero")
        if self.audit_logger is not None:
            self.capability_policy.audit_logger = self.audit_logger
            self.confirmation_policy.audit_logger = self.audit_logger

    @classmethod
    def connect(
        cls,
        target: str = "127.0.0.1:47777",
        memory_path: str | Path | None = None,
        capability_policy: CapabilityPolicy | None = None,
        capability_profile: str | None = None,
        confirmation_policy: ConfirmationPolicy | None = None,
        audit_log_path: str | Path | None = None,
        audit_source: str = "runtime",
        plugin_paths: tuple[str | Path, ...] = (),
        preflight_mode: str | None = None,
        preflight_timeout_seconds: float = 30.0,
    ) -> "AgentRuntime":
        if capability_policy is not None and capability_profile is not None:
            raise ValueError("use either capability_policy or capability_profile, not both")
        memory = SQLiteMemoryStore(memory_path) if memory_path is not None else MemoryStore()
        audit_logger = (
            JsonlAuditLogger(audit_log_path, source=audit_source)
            if audit_log_path is not None
            else None
        )
        if capability_policy is None:
            capability_policy = (
                CapabilityPolicy.from_profile(capability_profile, audit_logger=audit_logger)
                if capability_profile is not None
                else CapabilityPolicy.from_env(audit_logger=audit_logger)
            )
        return cls(
            client=PeekabooXClient(target=target),
            memory=memory,
            capability_policy=capability_policy,
            confirmation_policy=confirmation_policy or ConfirmationPolicy.disabled(),
            audit_logger=audit_logger,
            plugin_paths=tuple(Path(path) for path in plugin_paths),
            preflight_mode=preflight_mode,
            preflight_timeout_seconds=preflight_timeout_seconds,
        )

    def register_tool(self, name: str, tool: object) -> None:
        if not name:
            raise ValueError("tool name must not be empty")
        self.tools[name] = tool

    def list_plugins(
        self,
        paths: tuple[str | Path, ...] | list[str | Path] | None = None,
    ) -> PluginDiscoveryResult:
        self._require_capability(Capability.PLUGIN_READ, "list_plugins")
        selected_paths = tuple(Path(path) for path in paths) if paths is not None else self.plugin_paths
        result = discover_plugins(selected_paths if selected_paths else None)
        self.plugin_registry = result
        return result

    def call_plugin_tool(
        self,
        plugin_id: str,
        tool: str,
        arguments: dict[str, object] | None = None,
        *,
        paths: tuple[str | Path, ...] | list[str | Path] | None = None,
        timeout_seconds: float = 10.0,
        max_output_bytes: int = 1_048_576,
    ) -> PluginToolExecutionResult:
        self._require_capability(
            Capability.PLUGIN_EXECUTE,
            "call_plugin_tool",
            plugin_id=plugin_id,
            tool=tool,
        )
        plugins = self.list_plugins(paths=paths).plugins
        plugin = next((plugin for plugin in plugins if plugin.manifest.id == plugin_id), None)
        if plugin is None:
            raise ValueError(f"unknown plugin: {plugin_id}")
        return execute_plugin_tool(
            plugin,
            tool,
            dict(arguments or {}),
            timeout_seconds=timeout_seconds,
            max_output_bytes=max_output_bytes,
        )

    def plan(self, goal: str) -> list[str]:
        self._require_capability(Capability.WORKFLOW_GENERATE, "plan")
        return self.planner.decompose(goal)

    def plan_workflow(self, goal: str) -> Workflow:
        self._require_capability(Capability.WORKFLOW_GENERATE, "plan_workflow")
        return self.planner.plan_workflow(goal)

    def capability_audit(self) -> tuple[CapabilityAuditEvent, ...]:
        return tuple(self.capability_policy.audit_events)

    def confirmation_audit(self) -> tuple[ConfirmationAuditEvent, ...]:
        return tuple(self.confirmation_policy.audit_events)

    def preflight_audit(self) -> tuple[PreflightAuditEvent, ...]:
        return tuple(self.preflight_audit_events)

    def doctor(
        self,
        *,
        strict: bool = False,
        timeout_seconds: float = 30.0,
    ) -> DoctorResult:
        self._require_capability(Capability.OBSERVE, "doctor", strict=strict)
        result = run_doctor(strict=strict, timeout_seconds=timeout_seconds)
        self._preflight_doctor_result = result
        return result

    def preflight(
        self,
        categories: Sequence[str] | str,
        *,
        operation: str = "runtime",
        refresh: bool = False,
        timeout_seconds: float | None = None,
    ) -> PreflightResult:
        required_categories = _preflight_categories(categories)
        self._require_capability(
            Capability.OBSERVE,
            "preflight",
            target_operation=operation,
            categories=list(required_categories),
            refresh=refresh,
        )
        doctor = self._doctor_for_preflight(
            refresh=refresh,
            timeout_seconds=timeout_seconds,
        )
        result = _preflight_result(
            doctor,
            operation=operation,
            required_categories=required_categories,
        )
        self._record_preflight_audit(result)
        return result

    def require_preflight(
        self,
        categories: Sequence[str] | str,
        *,
        operation: str = "runtime",
        refresh: bool = False,
        timeout_seconds: float | None = None,
    ) -> PreflightResult:
        result = self.preflight(
            categories,
            operation=operation,
            refresh=refresh,
            timeout_seconds=timeout_seconds,
        )
        if not result.ok:
            raise PreflightError(result)
        return result

    def generate_workflow(
        self,
        goal: str,
        *,
        refresh_desktop_graph: bool = False,
    ) -> Workflow:
        self._require_capability(Capability.WORKFLOW_GENERATE, "generate_workflow")
        if refresh_desktop_graph:
            self.refresh_desktop_graph()
        desktop_nodes: tuple[GraphNode, ...] = ()
        if (
            self.memory.latest_desktop_snapshot() is not None
            and not self.memory.desktop_graph_stale
        ):
            desktop_nodes = self.query_desktop_graph(kind="element")
        return self.planner.generate_workflow(goal, desktop_nodes=desktop_nodes)

    def refine_workflow(
        self,
        goal: str,
        workflow: Workflow | None = None,
        *,
        refresh_desktop_graph: bool = False,
    ) -> Workflow:
        self._require_capability(Capability.WORKFLOW_GENERATE, "refine_workflow")
        if refresh_desktop_graph:
            self.refresh_desktop_graph()
        desktop_nodes: tuple[GraphNode, ...] = ()
        if (
            self.memory.latest_desktop_snapshot() is not None
            and not self.memory.desktop_graph_stale
        ):
            desktop_nodes = self.query_desktop_graph(kind="element")
        return self.planner.refine_workflow(
            goal,
            draft=workflow,
            desktop_nodes=desktop_nodes,
        )

    def replan_workflow(
        self,
        goal: str,
        failed_workflow: Workflow,
        failed_result: WorkflowExecutionResult,
        *,
        refresh_desktop_graph: bool = False,
    ) -> Workflow:
        self._require_capability(Capability.WORKFLOW_GENERATE, "replan_workflow")
        if refresh_desktop_graph:
            self.refresh_desktop_graph()
        desktop_nodes: tuple[GraphNode, ...] = ()
        if (
            self.memory.latest_desktop_snapshot() is not None
            and not self.memory.desktop_graph_stale
        ):
            desktop_nodes = self.query_desktop_graph(kind="element")
        failed_step_index = int(failed_result.recovery.get("failed_step", 0))
        return self.planner.replan_workflow(
            goal,
            failed_workflow=failed_workflow,
            failed_step_index=failed_step_index,
            reason=str(failed_result.recovery.get("reason", "workflow failed")),
            attempts=int(failed_result.recovery.get("attempts", 0)),
            desktop_nodes=desktop_nodes,
        )

    def save_generated_workflow(
        self,
        goal: str,
        path: str | Path,
        format_name: str | None = None,
        *,
        refresh_desktop_graph: bool = False,
    ) -> str:
        self._require_capability(Capability.WORKFLOW_GENERATE, "save_generated_workflow")
        workflow = self.generate_workflow(
            goal,
            refresh_desktop_graph=refresh_desktop_graph,
        )
        return str(save_workflow_file(workflow, path, format_name=format_name))

    def save_refined_workflow(
        self,
        goal: str,
        path: str | Path,
        workflow: Workflow | None = None,
        format_name: str | None = None,
        *,
        refresh_desktop_graph: bool = False,
    ) -> str:
        self._require_capability(Capability.WORKFLOW_GENERATE, "save_refined_workflow")
        refined = self.refine_workflow(
            goal,
            workflow=workflow,
            refresh_desktop_graph=refresh_desktop_graph,
        )
        return str(save_workflow_file(refined, path, format_name=format_name))

    def execute_goal(
        self,
        goal: str,
        verifier: Verifier | None = None,
        *,
        replan_on_failure: bool = True,
        max_replans: int = 1,
    ) -> WorkflowExecutionResult:
        self._require_capability(Capability.WORKFLOW_EXECUTE, "execute_goal")
        if max_replans < 0:
            raise ValueError("max_replans must be non-negative")
        workflow = self.plan_workflow(goal)
        all_steps: list[StepExecutionResult] = []
        replan_events: list[dict[str, object]] = []

        for replan_index in range(max_replans + 1):
            result = self.execute_workflow(workflow, verifier=verifier)
            all_steps.extend(result.steps)
            if result.ok:
                recovery = dict(result.recovery)
                if replan_events:
                    recovery["replanned"] = True
                    recovery["replans"] = replan_events
                return WorkflowExecutionResult(
                    goal=result.goal,
                    ok=True,
                    steps=tuple(all_steps),
                    recovery=recovery,
                )
            if result.recovery.get("retryable") is False and "preflight" in result.recovery:
                recovery = dict(result.recovery)
                if replan_events:
                    recovery["replanned"] = True
                    recovery["replans"] = replan_events
                return WorkflowExecutionResult(
                    goal=result.goal,
                    ok=False,
                    steps=tuple(all_steps),
                    recovery=recovery,
                )
            if not replan_on_failure or replan_index >= max_replans:
                recovery = dict(result.recovery)
                if replan_events:
                    recovery["replanned"] = True
                    recovery["replans"] = replan_events
                return WorkflowExecutionResult(
                    goal=result.goal,
                    ok=False,
                    steps=tuple(all_steps),
                    recovery=recovery,
                )

            try:
                next_workflow = self.replan_workflow(
                    goal,
                    failed_workflow=workflow,
                    failed_result=result,
                    refresh_desktop_graph=True,
                )
            except Exception as exc:
                recovery = dict(result.recovery)
                recovery["replan_error"] = f"{type(exc).__name__}: {exc}"
                return WorkflowExecutionResult(
                    goal=result.goal,
                    ok=False,
                    steps=tuple(all_steps),
                    recovery=recovery,
                )

            replan_events.append(
                {
                    "attempt": replan_index + 1,
                    "workflow": next_workflow.name,
                    "steps": len(next_workflow.steps),
                    "reason": result.recovery.get("reason", "workflow failed"),
                }
            )
            workflow = next_workflow

        raise RuntimeError("unreachable execute_goal replanning state")

    def execute_workflow(
        self,
        workflow: Workflow,
        verifier: Verifier | None = None,
    ) -> WorkflowExecutionResult:
        self._require_capability(
            Capability.WORKFLOW_EXECUTE,
            "execute_workflow",
            workflow=workflow.name,
        )
        if not workflow.steps:
            raise ValueError("workflow must contain at least one step")
        try:
            self._require_preflight(
                "execute_workflow",
                _workflow_preflight_categories(workflow),
            )
        except PreflightError as exc:
            return WorkflowExecutionResult(
                goal=workflow.name,
                ok=False,
                steps=(),
                recovery=_preflight_recovery(exc.result),
            )
        self._require_confirmation(
            DangerousAction.WORKFLOW_EXECUTE,
            "execute_workflow",
            workflow=workflow.name,
            steps=len(workflow.steps),
        )

        step_results: list[StepExecutionResult] = []
        for step in workflow.steps:
            result = self.execute_step(step, verifier=verifier)
            step_results.append(result)
            if not result.ok:
                recovery: dict[str, object] = {
                    "failed_step": len(step_results) - 1,
                    "action": step.action,
                    "reason": result.recovery.get("reason", "step failed"),
                    "attempts": result.recovery.get("attempts", 0),
                    "next_action": "inspect_state",
                }
                for key in (
                    "successful",
                    "strategy",
                    "strategies",
                    "events",
                    "preflight",
                ):
                    if key in result.recovery:
                        recovery[key] = result.recovery[key]
                if "preflight" in recovery:
                    recovery["retryable"] = False
                return WorkflowExecutionResult(
                    goal=workflow.name,
                    ok=False,
                    steps=tuple(step_results),
                    recovery=recovery,
                )

        return WorkflowExecutionResult(
            goal=workflow.name,
            ok=True,
            steps=tuple(step_results),
            recovery={},
        )

    def load_workflow_file(self, path: str | Path) -> Workflow:
        return load_workflow_file(path)

    def execute_workflow_file(
        self,
        path: str | Path,
        verifier: Verifier | None = None,
    ) -> WorkflowExecutionResult:
        self._require_capability(
            Capability.WORKFLOW_EXECUTE,
            "execute_workflow_file",
            path=str(path),
        )
        return self.execute_workflow(self.load_workflow_file(path), verifier=verifier)

    def start_recording(self, name: str = "recorded-workflow") -> WorkflowRecorder:
        self._require_capability(Capability.WORKFLOW_RECORD, "start_recording")
        if not name.strip():
            raise ValueError("recording name must not be empty")
        self.recorder = WorkflowRecorder(name=name)
        self.last_recording = None
        return self.recorder

    def stop_recording(self) -> Workflow:
        self._require_capability(Capability.WORKFLOW_RECORD, "stop_recording")
        workflow = self.recorded_workflow()
        self.last_recording = workflow
        self.recorder = None
        return workflow

    def recorded_workflow(self) -> Workflow:
        self._require_capability(Capability.WORKFLOW_RECORD, "recorded_workflow")
        if self.recorder is not None:
            return self.recorder.workflow()
        if self.last_recording is not None:
            return self.last_recording
        raise RuntimeError("no active or completed workflow recording")

    def save_recording(
        self,
        path: str | Path,
        format_name: str | None = None,
    ) -> str:
        self._require_capability(
            Capability.WORKFLOW_RECORD,
            "save_recording",
            path=str(path),
        )
        workflow = self.recorded_workflow()
        recorder = WorkflowRecorder(name=workflow.name, steps=list(workflow.steps))
        return str(recorder.save(path, format_name=format_name))

    def execute_step(
        self,
        step: WorkflowStep,
        verifier: Verifier | None = None,
    ) -> StepExecutionResult:
        self._require_capability(
            Capability.WORKFLOW_EXECUTE,
            "execute_step",
            action=step.action,
        )
        max_attempts = max(1, self.retries + 1)
        attempts: list[ActionAttempt] = []
        recovery_events: list[dict[str, object]] = []
        current_step = step

        for attempt_index in range(1, max_attempts + 1):
            current_step, attempt_recovery = self._prepare_replay_recovery(
                current_step,
                attempt_index=attempt_index,
            )
            if attempt_recovery:
                recovery_events.append(attempt_recovery)

            try:
                result = self._perform_step(current_step)
                verification = _with_recovery_metadata(
                    self._verify_step(current_step, result, verifier),
                    attempt_recovery,
                )
                ok = verification.ok
                message = verification.message
                error = None
            except Exception as exc:
                result = None
                metadata: dict[str, object] = {"exception": type(exc).__name__}
                if isinstance(exc, PreflightError):
                    metadata["preflight"] = _preflight_metadata(exc.result)
                if attempt_recovery:
                    metadata["recovery"] = attempt_recovery
                    metadata["recovery_strategy"] = attempt_recovery["strategy"]
                verification = VerificationResult(
                    ok=False,
                    message=str(exc),
                    metadata=metadata,
                )
                ok = False
                message = str(exc)
                error = f"{type(exc).__name__}: {exc}"

            attempts.append(
                ActionAttempt(
                    attempt=attempt_index,
                    ok=ok,
                    message=message,
                    result=result,
                    error=error,
                    verification=verification,
                    recovery=attempt_recovery,
                )
            )
            if ok:
                return StepExecutionResult(
                    step=step,
                    ok=True,
                    attempts=tuple(attempts),
                    result=result,
                    recovery=_step_recovery_report(
                        successful=True,
                        attempt=attempt_index,
                        events=recovery_events,
                    ),
                )

        last_attempt = attempts[-1]
        recovery = {
            "action": step.action,
            "reason": last_attempt.message,
            "attempts": len(attempts),
            "retryable": False,
            "next_action": "inspect_state",
            **_step_recovery_report(
                successful=False,
                attempt=len(attempts),
                events=recovery_events,
            ),
        }
        if (
            last_attempt.verification is not None
            and "preflight" in last_attempt.verification.metadata
        ):
            recovery["preflight"] = last_attempt.verification.metadata["preflight"]
            recovery["next_action"] = "run_doctor"
        return StepExecutionResult(
            step=step,
            ok=False,
            attempts=tuple(attempts),
            result=last_attempt.result,
            recovery=recovery,
        )

    def capture_screen(
        self,
        include_semantic_tree: bool = False,
        region: Rect | None = None,
        window_id: str | None = None,
        app: str | None = None,
        window_title: str | None = None,
        title_regex: str | None = None,
    ) -> CaptureScreenResult:
        self._require_capability(Capability.OBSERVE, "capture_screen")
        self._require_preflight("capture_screen", ("desktop", "capture"))
        region, window_id = self._capture_screen_target(
            region=region,
            window_id=window_id,
            app=app,
            window_title=window_title,
            title_regex=title_regex,
        )
        result = self._require_client().capture_screen(
            include_semantic_tree,
            region=region,
            window_id=window_id,
        )
        self._record_step(WorkflowStep(action="observe"))
        return result

    def _capture_screen_target(
        self,
        *,
        region: Rect | None,
        window_id: str | None,
        app: str | None,
        window_title: str | None,
        title_regex: str | None,
    ) -> tuple[Rect | None, str | None]:
        if not any(
            _clean_optional_string(value)
            for value in (window_id, app, window_title, title_regex)
        ):
            return region, None

        kwargs = _window_query_kwargs(
            id=window_id,
            app=app,
            title=window_title,
            title_regex=title_regex,
            focused=False,
            limit=1,
            sort="focused",
            backend=None,
            diagnose=False,
        )
        windows = self._require_client().list_windows_result(**kwargs).windows
        if not windows:
            raise RuntimeError("no window matched capture filters")
        window = windows[0]
        if window.bounds.width <= 0 or window.bounds.height <= 0:
            raise RuntimeError(f"window {window.id} has empty bounds")
        if region is None:
            return None, window.id
        return _window_relative_rect(window.bounds, region), None

    def capture_delta(
        self,
        stream_id: str = "default",
        reset: bool = False,
        region: Rect | None = None,
        window_id: str | None = None,
        per_channel_threshold: int | None = None,
        low_bandwidth: bool = True,
    ) -> CaptureDeltaResult:
        self._require_capability(Capability.OBSERVE, "capture_delta")
        self._require_preflight("capture_delta", ("desktop", "capture"))
        result = self._require_client().capture_delta(
            stream_id=stream_id,
            reset=reset,
            region=region,
            window_id=window_id,
            per_channel_threshold=per_channel_threshold,
            low_bandwidth=low_bandwidth,
        )
        self._record_step(WorkflowStep(action="observe"))
        return result

    def capture_backends(
        self,
        output: str | Path = "screenshot.png",
        region: Rect | None = None,
        diagnose: bool = False,
        probe: str = "none",
    ) -> CaptureBackendsResult:
        self._require_capability(Capability.OBSERVE, "capture_backends")
        if probe != "none":
            self._require_preflight("capture_backends", "capture")
        result = self._require_client().capture_backends(
            output=output,
            region=region,
            diagnose=diagnose,
            probe=probe,
        )
        self._record_step(WorkflowStep(action="observe"))
        return result

    def ocr_screen(
        self,
        region: Rect | None = None,
        language: str | None = None,
        **kwargs: object,
    ) -> OcrResult:
        self._require_capability(Capability.VISION, "ocr_screen")
        self._require_preflight("ocr_screen", ("capture", "ocr"))
        return self._require_client().ocr_screen(region=region, language=language, **kwargs)

    def ocr_region(
        self, region: Rect, language: str | None = None, **kwargs: object
    ) -> OcrResult:
        self._require_capability(Capability.VISION, "ocr_region")
        self._require_preflight("ocr_region", ("capture", "ocr"))
        return self._require_client().ocr_region(region, language, **kwargs)

    def compare_images(
        self,
        expected_image: bytes,
        actual_image: bytes,
        region: Rect | None = None,
        per_channel_threshold: int | None = None,
        max_changed_ratio: float | None = None,
    ) -> VisualDiffResult:
        self._require_capability(Capability.VISION, "compare_images")
        return self._require_client().compare_images(
            expected_image,
            actual_image,
            region,
            per_channel_threshold,
            max_changed_ratio,
        )

    def compare_image_files(
        self,
        expected_path: str,
        actual_path: str,
        region: Rect | None = None,
        per_channel_threshold: int | None = None,
        max_changed_ratio: float | None = None,
    ) -> VisualDiffResult:
        self._require_capability(
            Capability.VISION,
            "compare_image_files",
            expected_path=expected_path,
            actual_path=actual_path,
        )
        return self._require_client().compare_image_files(
            expected_path,
            actual_path,
            region,
            per_channel_threshold,
            max_changed_ratio,
        )

    def detect_ui_state(
        self,
        images: tuple[bytes, ...] | list[bytes],
        region: Rect | None = None,
        per_channel_threshold: int | None = None,
        stable_max_changed_ratio: float | None = None,
        loading_min_changed_ratio: float | None = None,
        required_stable_transitions: int | None = None,
    ) -> UiStateResult:
        self._require_capability(Capability.VISION, "detect_ui_state")
        return self._require_client().detect_ui_state(
            images,
            region,
            per_channel_threshold,
            stable_max_changed_ratio,
            loading_min_changed_ratio,
            required_stable_transitions,
        )

    def detect_ui_state_from_image_files(
        self,
        image_paths: tuple[str, ...] | list[str],
        region: Rect | None = None,
        per_channel_threshold: int | None = None,
        stable_max_changed_ratio: float | None = None,
        loading_min_changed_ratio: float | None = None,
        required_stable_transitions: int | None = None,
    ) -> UiStateResult:
        self._require_capability(Capability.VISION, "detect_ui_state_from_image_files")
        return self._require_client().detect_ui_state_from_image_files(
            image_paths,
            region,
            per_channel_threshold,
            stable_max_changed_ratio,
            loading_min_changed_ratio,
            required_stable_transitions,
        )

    def detect_ui_elements(
        self,
        image: bytes,
        region: Rect | None = None,
        edge_threshold: int | None = None,
        min_width: int | None = None,
        min_height: int | None = None,
        min_component_pixels: int | None = None,
        max_elements: int | None = None,
        merge_distance: int | None = None,
    ) -> DetectUiElementsResult:
        self._require_capability(Capability.VISION, "detect_ui_elements")
        return self._require_client().detect_ui_elements(
            image,
            region,
            edge_threshold,
            min_width,
            min_height,
            min_component_pixels,
            max_elements,
            merge_distance,
        )

    def detect_ui_elements_from_image_file(
        self,
        image_path: str,
        region: Rect | None = None,
        edge_threshold: int | None = None,
        min_width: int | None = None,
        min_height: int | None = None,
        min_component_pixels: int | None = None,
        max_elements: int | None = None,
        merge_distance: int | None = None,
    ) -> DetectUiElementsResult:
        self._require_capability(
            Capability.VISION,
            "detect_ui_elements_from_image_file",
            image_path=image_path,
        )
        return self._require_client().detect_ui_elements_from_image_file(
            image_path,
            region,
            edge_threshold,
            min_width,
            min_height,
            min_component_pixels,
            max_elements,
            merge_distance,
        )

    def probe_dmabuf(self, import_target: str = "compute") -> DmaBufProbeResult:
        self._require_capability(Capability.OBSERVE, "probe_dmabuf", import_target=import_target)
        self._require_preflight("probe_dmabuf", "capture")
        return self._require_client().probe_dmabuf(import_target)

    def list_windows(
        self,
        *,
        id: str | None = None,
        app: str | None = None,
        title: str | None = None,
        title_regex: str | None = None,
        focused: bool = False,
        limit: int | None = None,
        sort: str | None = None,
        backend: str | None = None,
        diagnose: bool = False,
    ) -> tuple[WindowInfo, ...]:
        self._require_capability(Capability.OBSERVE, "list_windows")
        if not diagnose:
            self._require_preflight("list_windows", "desktop")
        kwargs = _window_query_kwargs(
            id=id,
            app=app,
            title=title,
            title_regex=title_regex,
            focused=focused,
            limit=limit,
            sort=sort,
            backend=backend,
            diagnose=diagnose,
        )
        client = self._require_client()
        if kwargs:
            return client.list_windows(**kwargs)
        return client.list_windows()

    def list_windows_result(
        self,
        *,
        id: str | None = None,
        app: str | None = None,
        title: str | None = None,
        title_regex: str | None = None,
        focused: bool = False,
        limit: int | None = None,
        sort: str | None = None,
        backend: str | None = None,
        diagnose: bool = False,
    ) -> WindowListResult:
        self._require_capability(Capability.OBSERVE, "list_windows")
        if not diagnose:
            self._require_preflight("list_windows", "desktop")
        kwargs = _window_query_kwargs(
            id=id,
            app=app,
            title=title,
            title_regex=title_regex,
            focused=focused,
            limit=limit,
            sort=sort,
            backend=backend,
            diagnose=diagnose,
        )
        return self._require_client().list_windows_result(**kwargs)

    def get_desktop_state(self) -> DesktopState:
        self._require_capability(Capability.OBSERVE, "get_desktop_state")
        self._require_preflight("get_desktop_state", "desktop")
        return self._require_client().get_desktop_state()

    def desktop_focus(
        self,
        app: str,
        *,
        use_gnome_overview: bool = True,
        launch_if_needed: bool = True,
        wait_after_focus_ms: int = 1_000,
        overview_wait_ms: int = 800,
        window_title: str | None = None,
        window_id: str | None = None,
        verify: bool = False,
    ) -> DesktopActionResult:
        self._require_capability(Capability.OBSERVE, "desktop_focus", app=app)
        self._require_capability(Capability.CLICK, "desktop_focus", app=app)
        self._require_preflight("desktop_focus", ("desktop", "input"))
        self._require_confirmation(
            DangerousAction.CLICK,
            "desktop_focus",
            app=app,
            has_window_title=bool(window_title),
            has_window_id=bool(window_id),
            verify=verify,
        )
        return self._require_client().desktop_focus(
            app,
            use_gnome_overview=use_gnome_overview,
            launch_if_needed=launch_if_needed,
            wait_after_focus_ms=wait_after_focus_ms,
            overview_wait_ms=overview_wait_ms,
            window_title=window_title,
            window_id=window_id,
            verify=verify,
        )

    def desktop_locate(
        self,
        app: str,
        target: str,
        *,
        image_path: str | Path | None = None,
        prefer_accessibility: bool = True,
        window_title: str | None = None,
        window_id: str | None = None,
    ) -> DesktopLocateResult:
        self._require_capability(Capability.OBSERVE, "desktop_locate", app=app, target=target)
        self._require_capability(Capability.VISION, "desktop_locate", app=app, target=target)
        self._require_preflight("desktop_locate", ("desktop", "capture"))
        return self._require_client().desktop_locate(
            app,
            target,
            image_path=image_path,
            prefer_accessibility=prefer_accessibility,
            window_title=window_title,
            window_id=window_id,
        )

    def desktop_click(
        self,
        app: str,
        target: str,
        *,
        image_path: str | Path | None = None,
        prefer_accessibility: bool = True,
        window_title: str | None = None,
        window_id: str | None = None,
        button: str = "left",
        dry_run: bool = False,
        verify: bool = False,
    ) -> DesktopActionResult:
        self._require_capability(Capability.OBSERVE, "desktop_click", app=app, target=target)
        self._require_capability(Capability.VISION, "desktop_click", app=app, target=target)
        self._require_capability(Capability.CLICK, "desktop_click", app=app, target=target)
        self._require_preflight("desktop_click", ("desktop", "capture", "input"))
        if not dry_run:
            self._require_confirmation(
                DangerousAction.CLICK,
                "desktop_click",
                app=app,
                target=target,
                button=button,
                has_window_id=bool(window_id),
                verify=verify,
            )
        return self._require_client().desktop_click(
            app,
            target,
            image_path=image_path,
            prefer_accessibility=prefer_accessibility,
            window_title=window_title,
            window_id=window_id,
            button=button,
            dry_run=dry_run,
            verify=verify,
        )

    def desktop_drag(
        self,
        app: str,
        target: str,
        *,
        image_path: str | Path | None = None,
        prefer_accessibility: bool = True,
        window_title: str | None = None,
        window_id: str | None = None,
        button: str = "left",
        from_ratio: tuple[float, float] = (0.5, 0.5),
        to_ratio: tuple[float, float] = (0.5, 0.5),
        duration_ms: int = 250,
        dry_run: bool = False,
        verify: bool = False,
    ) -> DesktopActionResult:
        if duration_ms < 0:
            raise ValueError("duration_ms must be non-negative")
        self._require_capability(Capability.OBSERVE, "desktop_drag", app=app, target=target)
        self._require_capability(Capability.VISION, "desktop_drag", app=app, target=target)
        self._require_capability(Capability.CLICK, "desktop_drag", app=app, target=target)
        self._require_preflight("desktop_drag", ("desktop", "capture", "input"))
        if not dry_run:
            self._require_confirmation(
                DangerousAction.CLICK,
                "desktop_drag",
                app=app,
                target=target,
                button=button,
                duration_ms=duration_ms,
                has_window_id=bool(window_id),
                verify=verify,
            )
        return self._require_client().desktop_drag(
            app,
            target,
            image_path=image_path,
            prefer_accessibility=prefer_accessibility,
            window_title=window_title,
            window_id=window_id,
            button=button,
            from_ratio=from_ratio,
            to_ratio=to_ratio,
            duration_ms=duration_ms,
            dry_run=dry_run,
            verify=verify,
        )

    def desktop_type_into(
        self,
        app: str,
        target: str,
        text: str,
        *,
        image_path: str | Path | None = None,
        prefer_accessibility: bool = True,
        window_title: str | None = None,
        window_id: str | None = None,
        clear: bool = False,
        dry_run: bool = False,
        verify: bool = False,
    ) -> DesktopActionResult:
        self._require_capability(Capability.OBSERVE, "desktop_type_into", app=app, target=target)
        self._require_capability(Capability.VISION, "desktop_type_into", app=app, target=target)
        self._require_capability(Capability.CLICK, "desktop_type_into", app=app, target=target)
        self._require_capability(
            Capability.TYPE_TEXT,
            "desktop_type_into",
            app=app,
            target=target,
            text_length=len(text),
        )
        self._require_preflight("desktop_type_into", ("desktop", "capture", "input"))
        if not dry_run:
            self._require_confirmation(
                DangerousAction.TYPE_TEXT,
                "desktop_type_into",
                app=app,
                target=target,
                text_length=len(text),
                clear=clear,
                has_window_id=bool(window_id),
                verify=verify,
            )
        return self._require_client().desktop_type_into(
            app,
            target,
            text,
            image_path=image_path,
            prefer_accessibility=prefer_accessibility,
            window_title=window_title,
            window_id=window_id,
            clear=clear,
            dry_run=dry_run,
            verify=verify,
        )

    def desktop_assert(
        self,
        app: str,
        target: str,
        *,
        assertion: str = "present",
        expected_text: str | None = None,
        image_path: str | Path | None = None,
        prefer_accessibility: bool = True,
        window_title: str | None = None,
        window_id: str | None = None,
    ) -> DesktopActionResult:
        self._require_capability(Capability.OBSERVE, "desktop_assert", app=app, target=target)
        assertion_name = assertion.strip().casefold().replace("-", "_")
        required_categories: tuple[str, ...] = ("desktop",)
        if assertion_name in {
            "active",
            "not_active",
            "contains",
            "not_contains",
        }:
            self._require_capability(Capability.VISION, "desktop_assert", app=app, target=target)
            required_categories = ("desktop", "capture")
            if assertion_name in {"contains", "not_contains"}:
                required_categories = ("desktop", "capture", "ocr")
        self._require_preflight("desktop_assert", required_categories)
        return self._require_client().desktop_assert(
            app,
            target,
            assertion=assertion,
            expected_text=expected_text,
            image_path=image_path,
            prefer_accessibility=prefer_accessibility,
            window_title=window_title,
            window_id=window_id,
        )

    def ingest_desktop_snapshot(
        self,
        state: DesktopState | None = None,
        snapshot_id: str | None = None,
        captured_at_unix_ms: int | None = None,
    ) -> DesktopGraphSnapshot:
        self._require_capability(Capability.MEMORY_WRITE, "ingest_desktop_snapshot")
        if state is None:
            state = self.get_desktop_state()
        return self.memory.ingest_desktop_state(
            state,
            snapshot_id=snapshot_id,
            captured_at_unix_ms=captured_at_unix_ms,
        )

    def latest_desktop_snapshot(self) -> DesktopGraphSnapshot | None:
        self._require_capability(Capability.MEMORY_READ, "latest_desktop_snapshot")
        return self.memory.latest_desktop_snapshot()

    def record_desktop_event(
        self,
        *,
        kind: str,
        source: str = "runtime",
        target_id: str | None = None,
        payload: dict[str, object] | None = None,
        occurred_at_unix_ms: int | None = None,
        state: DesktopState | None = None,
        snapshot_id: str | None = None,
    ) -> DesktopGraphUpdate:
        self._require_capability(
            Capability.MEMORY_WRITE,
            "record_desktop_event",
            kind=kind,
        )
        return self.memory.record_desktop_event(
            kind=kind,
            source=source,
            target_id=target_id,
            payload=payload,
            occurred_at_unix_ms=occurred_at_unix_ms,
            state=state,
            snapshot_id=snapshot_id,
        )

    def desktop_graph_status(self) -> DesktopGraphStatus:
        self._require_capability(Capability.MEMORY_READ, "desktop_graph_status")
        return self.memory.desktop_graph_status()

    def refresh_desktop_graph(self, snapshot_id: str | None = None) -> DesktopGraphSnapshot:
        self._require_capability(Capability.MEMORY_WRITE, "refresh_desktop_graph")
        return self.ingest_desktop_snapshot(snapshot_id=snapshot_id)

    def query_desktop_graph(
        self,
        *,
        kind: str | None = None,
        label_contains: str | None = None,
        role: str | None = None,
        attribute_equals: dict[str, object] | None = None,
        contained_by: str | None = None,
        latest_only: bool = True,
        refresh_if_stale: bool = False,
    ) -> tuple[GraphNode, ...]:
        self._require_capability(Capability.MEMORY_READ, "query_desktop_graph")
        if refresh_if_stale and self.memory.desktop_graph_stale:
            self.refresh_desktop_graph()
        return self.memory.query_desktop_nodes(
            kind=kind,
            label_contains=label_contains,
            role=role,
            attribute_equals=attribute_equals,
            contained_by=contained_by,
            latest_only=latest_only,
        )

    def query_desktop_edges(
        self,
        *,
        source: str | None = None,
        target: str | None = None,
        kind: str | None = None,
        latest_only: bool = True,
    ) -> tuple[GraphEdge, ...]:
        self._require_capability(Capability.MEMORY_READ, "query_desktop_edges")
        return self.memory.query_desktop_edges(
            source=source,
            target=target,
            kind=kind,
            latest_only=latest_only,
        )

    def click(
        self,
        x: int | None = None,
        y: int | None = None,
        semantic_selector: str | None = None,
        vision_fallback: bool = False,
    ) -> ActionResult:
        self._require_capability(Capability.CLICK, "click")
        if vision_fallback:
            self._require_capability(Capability.VISION, "click.vision_fallback")
        if semantic_selector is not None and x is None and y is None:
            return self.click_selector(
                semantic_selector,
                vision_fallback=vision_fallback,
            )
        categories: tuple[str, ...] = ("input",)
        if semantic_selector is not None:
            categories = ("desktop", "input")
        if vision_fallback:
            categories = (*categories, "capture")
        self._require_preflight("click", categories)
        self._require_confirmation(
            DangerousAction.CLICK,
            "click",
            x=x,
            y=y,
            semantic_selector=semantic_selector,
            vision_fallback=vision_fallback,
        )
        recorded_step = self._recorded_click_step(
            x=x,
            y=y,
            semantic_selector=semantic_selector,
            vision_fallback=vision_fallback,
        )
        result = self._require_client().click(
            x=x,
            y=y,
            semantic_selector=semantic_selector,
            vision_fallback=vision_fallback,
        )
        if recorded_step is not None:
            self._record_step(recorded_step)
        return result

    def click_selector(self, selector: str, vision_fallback: bool = False) -> ActionResult:
        self._require_capability(
            Capability.CLICK,
            "click_selector",
            selector=selector,
        )
        if vision_fallback:
            self._require_capability(Capability.VISION, "click_selector.vision_fallback")
        categories: tuple[str, ...] = ("desktop", "input")
        if vision_fallback:
            categories = ("desktop", "input", "capture")
        self._require_preflight("click_selector", categories)
        self._require_confirmation(
            DangerousAction.CLICK,
            "click_selector",
            selector=selector,
            vision_fallback=vision_fallback,
        )
        cached = self.memory.find_cached_elements(selector)
        recorded_selector = selector
        if cached:
            target = _smallest_element(cached)
            if self.recorder is not None:
                recorded_selector = self._semantic_selector_for_element(target) or selector
            bounds = target.bounds
            result = self._require_client().click(
                x=bounds.x + bounds.width // 2,
                y=bounds.y + bounds.height // 2,
                vision_fallback=vision_fallback,
            )
        else:
            result = self._require_client().click_selector(
                selector,
                vision_fallback=vision_fallback,
            )
        self._record_step(
            WorkflowStep(
                action="click",
                selector=recorded_selector,
                vision_fallback=vision_fallback,
            )
        )
        return result

    def move_mouse(self, x: int, y: int) -> ActionResult:
        self._require_capability(Capability.CLICK, "move_mouse", x=x, y=y)
        self._require_preflight("move_mouse", "input")
        self._require_confirmation(DangerousAction.CLICK, "move_mouse", x=x, y=y)
        result = self._require_client().move_mouse(x, y)
        self._record_step(WorkflowStep(action="move_mouse", x=x, y=y))
        return result

    def drag(
        self,
        from_x: int,
        from_y: int,
        to_x: int,
        to_y: int,
        *,
        button: str = "left",
        duration_ms: int = 250,
    ) -> ActionResult:
        if duration_ms < 0:
            raise ValueError("duration_ms must be non-negative")
        button = button.strip().casefold()
        if button not in {"left", "middle", "right"}:
            raise ValueError("button must be left, middle, or right")
        self._require_capability(
            Capability.CLICK,
            "drag",
            from_x=from_x,
            from_y=from_y,
            to_x=to_x,
            to_y=to_y,
            button=button,
            duration_ms=duration_ms,
        )
        self._require_preflight("drag", "input")
        self._require_confirmation(
            DangerousAction.CLICK,
            "drag",
            from_x=from_x,
            from_y=from_y,
            to_x=to_x,
            to_y=to_y,
            button=button,
            duration_ms=duration_ms,
        )
        result = self._require_client().drag(
            from_x,
            from_y,
            to_x,
            to_y,
            button=button,
            duration_ms=duration_ms,
        )
        self._record_step(
            WorkflowStep(
                action="drag",
                from_x=from_x,
                from_y=from_y,
                to_x=to_x,
                to_y=to_y,
                button=button,
                duration_ms=duration_ms,
            )
        )
        return result

    def type_text(
        self,
        text: str,
        typing_speed_chars_per_second: int | None = None,
    ) -> ActionResult:
        self._require_capability(Capability.TYPE_TEXT, "type_text", text_length=len(text))
        self._require_preflight("type_text", "input")
        self._require_confirmation(
            DangerousAction.TYPE_TEXT,
            "type_text",
            text_length=len(text),
            typing_speed_chars_per_second=typing_speed_chars_per_second,
        )
        result = self._require_client().type_text(text, typing_speed_chars_per_second)
        self._record_step(WorkflowStep(action="type_text", value=text))
        return result

    def paste_text(self, text: str, preserve_clipboard: bool = False) -> ActionResult:
        self._require_capability(Capability.TYPE_TEXT, "paste_text", text_length=len(text))
        self._require_preflight("paste_text", "input")
        self._require_confirmation(
            DangerousAction.TYPE_TEXT,
            "paste_text",
            text_length=len(text),
            preserve_clipboard=preserve_clipboard,
        )
        result = self._require_client().paste_text(text, preserve_clipboard=preserve_clipboard)
        self._record_step(WorkflowStep(action="paste_text", value=text))
        return result

    def hotkey(self, keys: Sequence[str] | str) -> ActionResult:
        key_values = _hotkey_keys(keys)
        self._require_capability(Capability.CLICK, "hotkey", key_count=len(key_values))
        self._require_preflight("hotkey", "input")
        self._require_confirmation(
            DangerousAction.CLICK,
            "hotkey",
            key_count=len(key_values),
        )
        result = self._require_client().hotkey(key_values)
        self._record_step(WorkflowStep(action="hotkey", value="+".join(key_values)))
        return result

    def find_element(
        self,
        selector: str,
        vision_fallback: bool = False,
        app: str | None = None,
        window_title: str | None = None,
        window_id: str | None = None,
        vision_region: Rect | None = None,
        vision_edge_threshold: int | None = None,
        vision_min_width: int | None = None,
        vision_min_height: int | None = None,
        vision_min_component_pixels: int | None = None,
        vision_max_elements: int | None = None,
        vision_merge_distance: int | None = None,
    ) -> tuple[UiElement, ...]:
        self._require_capability(
            Capability.OBSERVE,
            "find_element",
            selector=selector,
        )
        if vision_fallback:
            self._require_capability(Capability.VISION, "find_element.vision_fallback")
        categories: tuple[str, ...] = ("desktop",)
        if vision_fallback:
            categories = ("desktop", "capture")
        self._require_preflight("find_element", categories)
        has_scope_or_vision_options = any(
            value is not None
            for value in (
                app,
                window_title,
                window_id,
                vision_region,
                vision_edge_threshold,
                vision_min_width,
                vision_min_height,
                vision_min_component_pixels,
                vision_max_elements,
                vision_merge_distance,
            )
        )
        cached = () if has_scope_or_vision_options else self.memory.find_cached_elements(selector)
        if cached:
            result = cached
        elif has_scope_or_vision_options:
            result = self._require_client().find_element(
                selector,
                vision_fallback=vision_fallback,
                app=app,
                window_title=window_title,
                window_id=window_id,
                vision_region=vision_region,
                vision_edge_threshold=vision_edge_threshold,
                vision_min_width=vision_min_width,
                vision_min_height=vision_min_height,
                vision_min_component_pixels=vision_min_component_pixels,
                vision_max_elements=vision_max_elements,
                vision_merge_distance=vision_merge_distance,
            )
        else:
            result = self._require_client().find_element(
                selector,
                vision_fallback=vision_fallback,
            )
        self._record_step(
            WorkflowStep(
                action="find_element",
                selector=selector,
                vision_fallback=vision_fallback,
            )
        )
        return result

    def _perform_step(self, step: WorkflowStep) -> object:
        action = step.action.strip().lower()
        if action in {"observe", "capture", "capture_screen"}:
            return self.capture_screen(include_semantic_tree=True)
        if action == "find_element":
            if not step.selector:
                raise ValueError("find_element step requires selector")
            return self.find_element(step.selector, vision_fallback=step.vision_fallback)
        if action == "click":
            if step.selector:
                return self.click_selector(step.selector, vision_fallback=step.vision_fallback)
            if step.x is None or step.y is None:
                raise ValueError("click step requires selector or x/y coordinates")
            return self.click(x=step.x, y=step.y, vision_fallback=step.vision_fallback)
        if action in {"move", "move_mouse"}:
            if step.x is None or step.y is None:
                raise ValueError("move_mouse step requires x/y coordinates")
            return self.move_mouse(step.x, step.y)
        if action == "drag":
            if (
                step.from_x is None
                or step.from_y is None
                or step.to_x is None
                or step.to_y is None
            ):
                raise ValueError("drag step requires from_x/from_y/to_x/to_y")
            return self.drag(
                step.from_x,
                step.from_y,
                step.to_x,
                step.to_y,
                button=step.button or "left",
                duration_ms=step.duration_ms if step.duration_ms is not None else 250,
            )
        if action in {"type", "type_text"}:
            if step.value is None:
                raise ValueError("type_text step requires value")
            return self.type_text(step.value)
        if action in {"paste", "paste_text"}:
            if step.value is None:
                raise ValueError("paste_text step requires value")
            return self.paste_text(step.value)
        if action == "hotkey":
            if step.value is None:
                raise ValueError("hotkey step requires value")
            return self.hotkey(step.value)
        if action == "list_windows":
            return self.list_windows()
        if action == "get_desktop_state":
            return self.get_desktop_state()
        raise ValueError(f"unsupported workflow action: {step.action}")

    def _verify_step(
        self,
        step: WorkflowStep,
        result: object,
        verifier: Verifier | None,
    ) -> VerificationResult:
        action = step.action.strip().lower()
        if isinstance(result, ActionResult) and not result.ok:
            return VerificationResult(
                ok=False,
                message=result.message or "action returned ok=false",
                metadata={"action_result": result.message},
            )

        if action == "find_element" and not result:
            return VerificationResult(
                ok=False,
                message="find_element returned no elements",
                metadata={"selector": step.selector or ""},
            )

        if verifier is not None:
            custom = verifier(step, result)
            if isinstance(custom, VerificationResult):
                return custom
            return VerificationResult(
                ok=bool(custom),
                message="custom verifier passed" if custom else "custom verifier failed",
            )

        if not step.verify:
            return VerificationResult(ok=True, message="verification skipped")

        if action in {
            "click",
            "move",
            "move_mouse",
            "drag",
            "type",
            "type_text",
            "paste",
            "paste_text",
            "hotkey",
        }:
            state = self.get_desktop_state()
            self.ingest_desktop_snapshot(state)
            return VerificationResult(
                ok=True,
                message="desktop state sampled after action",
                metadata={
                    "windows": len(state.windows),
                    "elements": len(state.elements),
                    "has_active_window": state.active_window is not None,
                },
            )

        return VerificationResult(ok=True, message="result accepted")

    def _doctor_for_preflight(
        self,
        *,
        refresh: bool = False,
        timeout_seconds: float | None = None,
    ) -> DoctorResult:
        if not refresh and self._preflight_doctor_result is not None:
            return self._preflight_doctor_result
        timeout = (
            timeout_seconds
            if timeout_seconds is not None
            else self.preflight_timeout_seconds
        )
        if timeout <= 0:
            raise ValueError("preflight timeout_seconds must be greater than zero")
        result = run_doctor(strict=False, timeout_seconds=timeout)
        self._preflight_doctor_result = result
        return result

    def _require_preflight(
        self,
        operation: str,
        categories: Sequence[str] | str,
    ) -> PreflightResult | None:
        if self.preflight_mode == "off":
            return None
        required_categories = _preflight_categories(categories)
        if not required_categories:
            return None
        result = self.preflight(required_categories, operation=operation)
        if self.preflight_mode == "strict" and not result.ok:
            raise PreflightError(result)
        return result

    def _record_preflight_audit(self, result: PreflightResult) -> None:
        status = _preflight_status(result)
        metadata = _preflight_metadata(result)
        metadata["mode"] = self.preflight_mode
        metadata["enforced"] = self.preflight_mode == "strict"
        event = PreflightAuditEvent(
            operation=result.operation,
            status=status,
            mode=self.preflight_mode,
            ok=result.ok,
            occurred_at_unix_ms=_unix_ms(),
            required_categories=result.required_categories,
            blocked_categories=result.blocked_categories,
            warning_categories=result.warning_categories,
            metadata=metadata,
        )
        self.preflight_audit_events.append(event)
        if self.audit_logger is not None:
            self.audit_logger.write(
                "preflight",
                status,
                metadata,
                error=None if result.ok else _preflight_error_message(result),
            )

    def _require_client(self) -> PeekabooXClient:
        if self.client is None:
            raise RuntimeError("AgentRuntime requires a PeekabooXClient for daemon RPC calls")
        return self.client

    def _require_capability(
        self,
        capability: str,
        operation: str,
        **metadata: object,
    ) -> None:
        self.capability_policy.require(capability, operation, metadata)

    def _require_confirmation(
        self,
        action: str,
        operation: str,
        **metadata: object,
    ) -> None:
        self.confirmation_policy.confirm(action, operation, metadata)

    def _record_step(self, step: WorkflowStep) -> None:
        if self.recorder is not None:
            self.recorder.record_step(step)

    def _prepare_replay_recovery(
        self,
        step: WorkflowStep,
        *,
        attempt_index: int,
    ) -> tuple[WorkflowStep, dict[str, object]]:
        if attempt_index <= 1 or not _is_selector_replay_step(step):
            return step, {}

        if attempt_index == 2:
            try:
                snapshot = self.refresh_desktop_graph()
            except Exception as exc:
                return step, {
                    "strategy": "refresh_desktop_graph",
                    "ok": False,
                    "error": f"{type(exc).__name__}: {exc}",
                }
            return step, {
                "strategy": "refresh_desktop_graph",
                "ok": True,
                "snapshot_id": snapshot.id,
            }

        if not step.vision_fallback:
            return replace(step, vision_fallback=True), {
                "strategy": "vision_fallback",
                "ok": True,
            }

        return step, {}

    def _recorded_click_step(
        self,
        *,
        x: int | None,
        y: int | None,
        semantic_selector: str | None,
        vision_fallback: bool,
    ) -> WorkflowStep | None:
        if self.recorder is None:
            return None
        if semantic_selector is not None:
            return WorkflowStep(
                action="click",
                selector=semantic_selector,
                vision_fallback=vision_fallback,
            )
        if x is None or y is None:
            return None

        try:
            selector = self._semantic_selector_for_point(x, y)
        except Exception:
            selector = None
        if selector is not None:
            return WorkflowStep(
                action="click",
                selector=selector,
                vision_fallback=vision_fallback,
            )
        return WorkflowStep(
            action="click",
            x=x,
            y=y,
            vision_fallback=vision_fallback,
        )

    def _semantic_selector_for_point(self, x: int, y: int) -> str | None:
        if self.memory.latest_desktop_snapshot() is None or self.memory.desktop_graph_stale:
            try:
                self.refresh_desktop_graph()
            except Exception:
                return None

        matches = self.memory.find_cached_elements(f"contains={x},{y}")
        if not matches:
            return None
        return self._semantic_selector_for_element(_smallest_element(matches))

    def _semantic_selector_for_element(self, element: UiElement) -> str | None:
        for selector in _candidate_selectors_for_element(element):
            matches = self.memory.find_cached_elements(selector)
            if len(matches) == 1 and matches[0].id == element.id:
                return selector
        return None


def _candidate_selectors_for_element(element: UiElement) -> tuple[str, ...]:
    role = _selector_value(element.role)
    label = _selector_value(element.label)
    states = tuple(
        state
        for state in (_selector_value(state) for state in element.states)
        if state is not None
    )
    bounds = _bounds_selector(element)
    selectors: list[str] = []

    def add(*parts: str | None) -> None:
        selector_parts = [part for part in parts if part is not None]
        if selector_parts:
            selectors.append(",".join(selector_parts))

    role_part = f"role={role}" if role is not None else None
    label_part = f"label={label}" if label is not None else None

    add(role_part, label_part)
    for state in states:
        add(role_part, label_part, f"state={state}")
    add(label_part)
    for state in states:
        add(label_part, f"state={state}")
    add(role_part)
    for state in states:
        add(role_part, f"state={state}")
    add(role_part, label_part, bounds)
    add(label_part, bounds)
    add(role_part, bounds)
    add(bounds)

    return tuple(dict.fromkeys(selectors))


def _normalize_preflight_mode(value: str | None) -> str:
    raw = value
    if raw is None:
        raw = os.environ.get("PEEKABOOX_PREFLIGHT_MODE", "off")
    normalized = raw.strip().casefold().replace("_", "-")
    aliases = {
        "0": "off",
        "false": "off",
        "disabled": "off",
        "disable": "off",
        "none": "off",
        "no": "off",
        "1": "strict",
        "true": "strict",
        "enabled": "strict",
        "enable": "strict",
        "on": "strict",
        "require": "strict",
        "required": "strict",
        "block": "strict",
        "blocking": "strict",
        "audit": "warn",
        "warning": "warn",
    }
    normalized = aliases.get(normalized, normalized)
    if normalized not in {"off", "warn", "strict"}:
        raise ValueError("preflight_mode must be off, warn, or strict")
    return normalized


def _preflight_categories(categories: Sequence[str] | str) -> tuple[str, ...]:
    values = (categories,) if isinstance(categories, str) else tuple(categories)
    normalized: list[str] = []
    for value in values:
        category = value.strip().casefold().replace("_", "-")
        if not category:
            raise ValueError("preflight categories must not be empty")
        if category not in normalized:
            normalized.append(category)
    return tuple(normalized)


def _preflight_result(
    doctor: DoctorResult,
    *,
    operation: str,
    required_categories: tuple[str, ...],
) -> PreflightResult:
    category_status = {category.name: category.status for category in doctor.categories}
    category_severity = {category.name: category.severity for category in doctor.categories}
    blocked: list[str] = []
    warnings: list[str] = []
    messages: list[str] = []

    for category in required_categories:
        status = category_status.get(category)
        if status is None:
            blocked.append(category)
            messages.append(f"{category}: missing doctor category")
            continue
        if status == "fail":
            blocked.append(category)
            messages.append(f"{category}: {_doctor_category_detail(doctor, category)}")
        elif status == "warn":
            warnings.append(category)
            messages.append(f"{category}: {_doctor_category_detail(doctor, category)}")

    return PreflightResult(
        operation=operation,
        required_categories=required_categories,
        ok=not blocked,
        blocked_categories=tuple(blocked),
        warning_categories=tuple(warnings),
        category_status=category_status,
        category_severity=category_severity,
        messages=tuple(messages),
        doctor_status=doctor.status,
    )


def _doctor_category_detail(doctor: DoctorResult, category: str) -> str:
    checks = [
        f"{check.name}={check.status}: {check.detail}"
        for check in doctor.checks
        if check.category == category and check.status != "ok"
    ]
    if checks:
        return "; ".join(checks[:3])
    summary = next((item for item in doctor.categories if item.name == category), None)
    if summary is None:
        return "category missing"
    return summary.status


def _preflight_error_message(result: PreflightResult) -> str:
    categories = ", ".join(result.blocked_categories) or ", ".join(result.required_categories)
    detail = (
        "; ".join(result.messages)
        if result.messages
        else "doctor reported unavailable support"
    )
    return f"preflight blocked {result.operation}: {categories}; {detail}"


def _preflight_status(result: PreflightResult) -> str:
    if not result.ok:
        return "blocked"
    if result.warning_categories:
        return "warning"
    return "ok"


def _preflight_metadata(result: PreflightResult) -> dict[str, object]:
    return {
        "operation": result.operation,
        "required_categories": list(result.required_categories),
        "blocked_categories": list(result.blocked_categories),
        "warning_categories": list(result.warning_categories),
        "category_status": dict(result.category_status),
        "category_severity": dict(result.category_severity),
        "messages": list(result.messages),
        "doctor_status": result.doctor_status,
    }


def _preflight_recovery(result: PreflightResult) -> dict[str, object]:
    return {
        "reason": _preflight_error_message(result),
        "retryable": False,
        "next_action": "run_doctor",
        "preflight": _preflight_metadata(result),
    }


def _workflow_preflight_categories(workflow: Workflow) -> tuple[str, ...]:
    categories: list[str] = []

    def add(*values: str) -> None:
        for value in values:
            if value not in categories:
                categories.append(value)

    for step in workflow.steps:
        action = step.action.strip().lower()
        if action in {"observe", "capture", "capture_screen"}:
            add("desktop", "capture")
        elif action == "find_element":
            add("desktop")
            if step.vision_fallback:
                add("capture")
        elif action == "click":
            add("input")
            if step.selector:
                add("desktop")
            if step.vision_fallback:
                add("capture")
        elif action in {
            "move",
            "move_mouse",
            "drag",
            "type",
            "type_text",
            "paste",
            "paste_text",
            "hotkey",
        }:
            add("input")
        elif action in {"list_windows", "get_desktop_state"}:
            add("desktop")
    return tuple(categories)


def _unix_ms() -> int:
    return int(time.time() * 1000)


def _hotkey_keys(keys: Sequence[str] | str) -> list[str]:
    if isinstance(keys, str):
        key_values = [part.strip() for part in keys.split("+")]
    else:
        key_values = [str(key).strip() for key in keys]
    key_values = [key for key in key_values if key]
    if not key_values:
        raise ValueError("hotkey requires at least one key")
    return key_values


def _selector_value(value: str | None) -> str | None:
    if value is None:
        return None
    value = value.strip()
    if not value or any(separator in value for separator in ",\n\r"):
        return None
    return value


def _bounds_selector(element: UiElement) -> str:
    bounds = element.bounds
    return f"bounds={bounds.x},{bounds.y},{bounds.width},{bounds.height}"


def _smallest_element(elements: tuple[UiElement, ...]) -> UiElement:
    return min(elements, key=lambda element: element.bounds.width * element.bounds.height)


def _is_selector_replay_step(step: WorkflowStep) -> bool:
    action = step.action.strip().lower()
    return bool(step.selector) and action in {"click", "find_element"}


def _with_recovery_metadata(
    verification: VerificationResult,
    recovery: dict[str, object],
) -> VerificationResult:
    if not recovery:
        return verification
    metadata = dict(verification.metadata)
    metadata["recovery"] = recovery
    metadata["recovery_strategy"] = recovery["strategy"]
    return VerificationResult(
        ok=verification.ok,
        message=verification.message,
        metadata=metadata,
    )


def _step_recovery_report(
    *,
    successful: bool,
    attempt: int,
    events: list[dict[str, object]],
) -> dict[str, object]:
    if not events:
        return {}
    strategies = [str(event["strategy"]) for event in events]
    report: dict[str, object] = {
        "successful": successful,
        "attempt": attempt,
        "strategies": strategies,
        "events": list(events),
    }
    successful_events = [event for event in events if event.get("ok", False)]
    if successful and successful_events:
        report["strategy"] = successful_events[-1]["strategy"]
    return report


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="peekaboox-agent",
        description="Inspect a local PeekabooX daemon or plugin installation.",
    )
    parser.add_argument("--version", action="store_true", help="print package version and exit")
    parser.add_argument(
        "--target",
        default=DEFAULT_GRPC_TARGET,
        help=f"daemon gRPC target, default: {DEFAULT_GRPC_TARGET}",
    )
    parser.add_argument(
        "--profile",
        choices=KNOWN_CAPABILITY_PROFILES,
        default="observe",
        help="capability profile for daemon/plugin operations",
    )
    parser.add_argument("--audit-log", help="optional JSONL audit log path")
    parser.add_argument(
        "--preflight-mode",
        choices=("off", "warn", "strict"),
        help="Doctor-backed preflight mode for live operations",
    )
    parser.add_argument(
        "--preflight-timeout",
        type=_positive_float,
        default=30.0,
        help="maximum seconds to wait for preflight Doctor checks",
    )
    parser.add_argument(
        "--plugin-path",
        action="append",
        default=[],
        help="plugin search path, repeatable",
    )
    subparsers = parser.add_subparsers(dest="command")
    windows_parser = subparsers.add_parser("windows", help="list desktop windows through the daemon")
    windows_parser.add_argument("--id", help="filter by exact window id")
    windows_parser.add_argument("--app", help="filter by app id/name or title substring")
    windows_parser.add_argument("--title", help="filter by title substring")
    windows_parser.add_argument("--title-regex", help="filter by title regular expression")
    windows_parser.add_argument("--focused", action="store_true", help="return focused windows only")
    windows_parser.add_argument("--limit", type=_positive_int, help="maximum number of windows")
    windows_parser.add_argument(
        "--sort",
        choices=WINDOW_SORT_CHOICES,
        help="window sort order",
    )
    windows_parser.add_argument(
        "--backend",
        choices=WINDOW_BACKEND_CHOICES,
        help="window backend to use",
    )
    windows_parser.add_argument(
        "--diagnose",
        action="store_true",
        help="include backend metadata, warnings, and diagnostic reports",
    )
    subparsers.add_parser("desktop-state", help="print daemon desktop state")
    plugins_parser = subparsers.add_parser("plugins", help="discover local plugins")
    plugins_parser.add_argument("--path", action="append", default=[], help="plugin search path")
    doctor_parser = subparsers.add_parser("doctor", help="run environment diagnostics")
    doctor_parser.add_argument("--strict", action="store_true", help="preserve strict doctor exit code")
    doctor_parser.add_argument(
        "--timeout",
        type=float,
        default=30.0,
        help="maximum seconds to wait for the doctor command",
    )
    preflight_parser = subparsers.add_parser(
        "preflight",
        help="check Doctor categories before live automation",
    )
    preflight_parser.add_argument(
        "categories",
        nargs="+",
        choices=("desktop", "capture", "input", "ocr", "python"),
        help="Doctor categories required by the planned operation",
    )
    preflight_parser.add_argument(
        "--operation",
        default="agent",
        help="operation name to include in the preflight result",
    )
    preflight_parser.add_argument(
        "--refresh",
        action="store_true",
        help="refresh Doctor diagnostics instead of using a cached result",
    )
    preflight_parser.add_argument(
        "--require",
        action="store_true",
        help="return exit code 1 when required categories are blocked",
    )
    preflight_parser.add_argument(
        "--timeout",
        type=_positive_float,
        default=None,
        help="maximum seconds to wait for this preflight Doctor check",
    )

    args = parser.parse_args(list(argv) if argv is not None else None)
    if args.version:
        print(f"peekaboox-agent {PEEKABOOX_VERSION}")
        return 0
    if args.command is None:
        parser.print_help()
        return 0

    try:
        if args.command == "plugins":
            paths = tuple(Path(path) for path in [*args.plugin_path, *args.path])
            runtime = _local_runtime(
                args.profile,
                args.audit_log,
                paths,
                preflight_mode=args.preflight_mode,
                preflight_timeout_seconds=args.preflight_timeout,
            )
            _print_json(runtime.list_plugins())
            return 0
        if args.command == "doctor":
            runtime = _local_runtime(
                args.profile,
                args.audit_log,
                tuple(Path(path) for path in args.plugin_path),
                preflight_mode=args.preflight_mode,
                preflight_timeout_seconds=args.preflight_timeout,
            )
            result = runtime.doctor(strict=args.strict, timeout_seconds=args.timeout)
            _print_json(result)
            return 1 if args.strict and result.fail_count else 0
        if args.command == "preflight":
            runtime = _local_runtime(
                args.profile,
                args.audit_log,
                tuple(Path(path) for path in args.plugin_path),
                preflight_mode=args.preflight_mode,
                preflight_timeout_seconds=args.preflight_timeout,
            )
            result = runtime.preflight(
                tuple(args.categories),
                operation=args.operation,
                refresh=args.refresh,
                timeout_seconds=args.timeout,
            )
            _print_json(result)
            return 1 if args.require and not result.ok else 0

        runtime = AgentRuntime.connect(
            target=args.target,
            capability_profile=args.profile,
            audit_log_path=args.audit_log,
            plugin_paths=tuple(Path(path) for path in args.plugin_path),
            preflight_mode=args.preflight_mode,
            preflight_timeout_seconds=args.preflight_timeout,
        )
        if args.command == "windows":
            query = _window_query_kwargs(
                id=args.id,
                app=args.app,
                title=args.title,
                title_regex=args.title_regex,
                focused=args.focused,
                limit=args.limit,
                sort=args.sort,
                backend=args.backend,
                diagnose=args.diagnose,
            )
            if args.diagnose:
                _print_json(runtime.list_windows_result(**query))
            else:
                _print_json(runtime.list_windows(**query))
            return 0
        if args.command == "desktop-state":
            _print_json(runtime.get_desktop_state())
            return 0
    except Exception as error:
        print(f"peekaboox-agent failed: {error}", file=sys.stderr)
        return 1

    parser.error(f"unknown command: {args.command}")
    return 2


def _window_query_kwargs(
    *,
    id: str | None,
    app: str | None,
    title: str | None,
    title_regex: str | None,
    focused: bool,
    limit: int | None,
    sort: str | None,
    backend: str | None,
    diagnose: bool,
) -> dict[str, object]:
    kwargs: dict[str, object] = {}
    for key, value in {
        "id": id,
        "app": app,
        "title": title,
        "title_regex": title_regex,
        "sort": sort,
        "backend": backend,
    }.items():
        value = _clean_optional_string(value)
        if value is not None:
            kwargs[key] = value
    if focused:
        kwargs["focused"] = focused
    if limit is not None:
        if limit <= 0:
            raise ValueError("limit must be greater than zero")
        kwargs["limit"] = limit
    if diagnose:
        kwargs["diagnose"] = diagnose
    return kwargs


def _clean_optional_string(value: str | None) -> str | None:
    if value is None:
        return None
    value = value.strip()
    return value or None


def _window_relative_rect(origin: Rect, region: Rect) -> Rect:
    if region.x < 0 or region.y < 0:
        raise ValueError("window-relative capture region must start inside the window")
    if region.x + region.width > origin.width or region.y + region.height > origin.height:
        raise ValueError("window-relative capture region must fit inside the window")
    return Rect(
        x=origin.x + region.x,
        y=origin.y + region.y,
        width=region.width,
        height=region.height,
    )


def _positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return parsed


def _positive_float(value: str) -> float:
    parsed = float(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return parsed


def _local_runtime(
    profile: str,
    audit_log_path: str | None,
    plugin_paths: tuple[Path, ...],
    *,
    preflight_mode: str | None = None,
    preflight_timeout_seconds: float = 30.0,
) -> AgentRuntime:
    audit_logger = (
        JsonlAuditLogger(audit_log_path, source="runtime")
        if audit_log_path is not None
        else None
    )
    return AgentRuntime(
        capability_policy=CapabilityPolicy.from_profile(profile, audit_logger=audit_logger),
        audit_logger=audit_logger,
        plugin_paths=plugin_paths,
        preflight_mode=preflight_mode,
        preflight_timeout_seconds=preflight_timeout_seconds,
    )


def _print_json(value: object) -> None:
    print(json.dumps(_to_json_value(value), indent=2, sort_keys=True))


def _to_json_value(value: object) -> object:
    if is_dataclass(value):
        return {
            field.name: _to_json_value(getattr(value, field.name))
            for field in fields(value)
        }
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    if isinstance(value, tuple | list):
        return [_to_json_value(item) for item in value]
    if isinstance(value, dict):
        return {str(key): _to_json_value(item) for key, item in value.items()}
    if isinstance(value, Path):
        return str(value)
    return value


if __name__ == "__main__":
    raise SystemExit(main())
