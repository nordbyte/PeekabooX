# Python Runtime, Workflows, Memory, and MCP

This page keeps the Python and agent-facing runtime details that used to live
in the root README.

## Python Runtime Client

Use the Python runtime client against a running daemon:

```python
from peekaboox.agent import AgentRuntime
from peekaboox.client import Rect
from peekaboox.security import (
    CapabilityProfile,
    ConfirmationPolicy,
    DangerousAction,
)

runtime = AgentRuntime.connect(
    capability_profile=CapabilityProfile.ASSIST,
    confirmation_policy=ConfirmationPolicy.require_for([DangerousAction.CLICK]),
    audit_log_path="peekaboox-runtime-audit.jsonl",
)
print(runtime.list_windows())
print(runtime.list_windows(focused=True, limit=1, sort="focused"))
print(runtime.list_windows_result(app="calculator", diagnose=True))
print(runtime.doctor().status)
print(runtime.doctor().categories)
print(runtime.find_element("role=push button"))
print(runtime.ocr_screen().text)
print(
    runtime.capture_delta(
        stream_id="agent-loop",
        region=Rect(x=10, y=20, width=400, height=240),
    ).changed_bounds
)
print(runtime.capture_backends(output="screen.png", diagnose=True, probe="frame").probes)
print(runtime.compare_image_files("before.png", "after.png").matches)
print(runtime.detect_ui_state_from_image_files(["frame1.png", "frame2.png"]).state)
print(runtime.detect_ui_elements_from_image_file("screenshot.png").elements)
print(runtime.desktop_locate("telegram", "search-input"))
runtime.desktop_click("telegram", "search-input", dry_run=True)
runtime.desktop_type_into("telegram", "message-input", "PeekabooX", dry_run=True)
runtime.click_selector("role=push button,label=Submit", vision_fallback=True)
runtime.move_mouse(100, 200)
runtime.drag(100, 200, 320, 240, duration_ms=350)
runtime.hotkey(["ctrl", "s"])
```

The Python runtime and MCP tool surface share granular capability profiles:
`observe`, `plan`, `assist`, and `operator`. Denied capabilities raise
`CapabilityDeniedError` in Python and return MCP tool errors for JSON-RPC
callers, with in-memory audit events available through
`runtime.capability_audit()`. The daemon's separate `--profile operator` or
`--allow-input` gate still controls real input injection.

An optional `ConfirmationPolicy` can require application-provided confirmation
before dangerous `click`, `type_text`, `paste_text`, or `execute_workflow`
operations. Pointer movement, drags, and hotkeys use the `click` confirmation
gate. Decisions are available through `runtime.confirmation_audit()`. Pass
`audit_log_path` or run `peekaboox-mcp --audit-log <path>` to persist runtime
security checks as JSONL.

## Workflows

The runtime has a deterministic workflow execution loop. `WorkflowStep` actions
such as `find_element`, `click`, `move_mouse`, `drag`, `hotkey`, `type_text`,
`paste_text`, and `observe` are retried according to `AgentRuntime.retries`,
verified after execution, and return structured attempt and recovery metadata:

```python
from peekaboox.workflows import Workflow, WorkflowStep

workflow = Workflow(
    name="submit",
    steps=[
        WorkflowStep(action="find_element", selector="role=push button,label=Submit"),
        WorkflowStep(action="click", selector="role=push button,label=Submit", vision_fallback=True),
    ],
)
result = runtime.execute_workflow(workflow)
print(result.ok, result.recovery)
```

Editable workflow drafts can be generated from a goal. When a fresh semantic
desktop graph is available, the generator uses it to produce stronger
selectors:

```python
runtime.ingest_desktop_snapshot()
draft = runtime.generate_workflow("Click Submit and type 'Hello'")
runtime.save_generated_workflow("Click Submit and type 'Hello'", "generated.yaml")
```

Projects can attach a structured refinement provider to `PlanningEngine`. The
provider may improve a draft, but PeekabooX only accepts returned `Workflow`
objects or JSON/YAML workflow definitions that validate as supported
`WorkflowStep` sequences. A separate replanning provider can return a validated
replacement workflow after `execute_goal` fails:

```python
refined = runtime.refine_workflow("Click Submit and type 'Hello'")
runtime.save_refined_workflow("Click Submit and type 'Hello'", "refined.yaml")
```

During replay, selector-based `find_element` and `click` steps self-heal across
retries. After an initial selector failure, the runtime refreshes the semantic
desktop graph; on a later retry it enables `vision_fallback` if the step did
not already request it. Step results report the applied recovery strategies.

Workflows can also be loaded from JSON or YAML files. The checked-in
`examples/workflow.yaml` uses the same `WorkflowStep` fields as the Python API:

```python
result = runtime.execute_workflow_file("examples/workflow.yaml")
print(result.ok)
```

Interactive actions can be recorded into the same workflow format and exported
as JSON or YAML:

```python
runtime.start_recording("manual-submit")
runtime.find_element("role=push button,label=Submit")
runtime.click_selector("role=push button,label=Submit", vision_fallback=True)
runtime.type_text("Hello")
runtime.save_recording("recordings/manual-submit.yaml")
```

`find_element` also accepts daemon-scoped element lookup fields such as
`app`, `window_title`, `window_id`, and the `vision_*` fallback detector tuning
arguments. Scoped or vision-tuned lookups bypass stale graph cache hits and go
to the daemon so the requested window and detector options are honored.

When recording coordinate clicks, the runtime samples semantic desktop state if
needed and stores a stable selector such as `role=push button,label=Submit`
when the clicked point resolves to a unique element. Replay can then use the
element's current bounds instead of the original click coordinates.

## Semantic Desktop Graph

The runtime keeps a semantic desktop graph in memory. A desktop state snapshot
turns windows, UI elements, and containment relationships into a queryable
graph. Use `SQLiteMemoryStore` or `AgentRuntime.connect(memory_path=...)` to
persist memory values and graph snapshots across runs:

```python
from peekaboox.memory import SQLiteMemoryStore

runtime = AgentRuntime.connect(memory_path="peekaboox-memory.sqlite3")
snapshot = runtime.ingest_desktop_snapshot()
print(snapshot.active_window_id)
print(runtime.query_desktop_graph(kind="element", label_contains="submit", contained_by="window-1"))
graph_json = runtime.memory.export_desktop_graph()
```

Desktop events can invalidate or refresh that graph. Events without a fresh
state mark the graph stale; `refresh_if_stale=True` samples the daemon before
serving a query:

```python
runtime.record_desktop_event(kind="window.focused", source="accessibility", target_id="window-1")
print(runtime.desktop_graph_status().stale)
print(runtime.query_desktop_graph(kind="element", refresh_if_stale=True))
```

Fresh graph snapshots are also used as a semantic lookup cache. `find_element`
and semantic `click_selector` first match selectors against cached graph
elements, and only fall back to daemon semantic lookup when the graph is stale
or has no match.

## MCP Server

`peekaboox-mcp` exposes a concrete MCP-style tool registry and dispatcher over
the Python runtime. Run it as a stdio MCP server after installing the Python
package, or directly from the checkout during development:

```bash
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --list-tools
PYTHONPATH=python/src python3 -m peekaboox.mcp.server
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --audit-log runtime-audit.jsonl
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --capability-profile observe
```

Tool execution through MCP requires Python runtime dependencies and a running
`peekabooxd` reachable at `PEEKABOOX_GRPC_TARGET` or `--target`. Without those
dependencies, the server can still list tool descriptors for inspection.

The current tool surface includes capture, capture delta, DMA-BUF probe,
click, text and paste input, semantic lookup, window listing, desktop state,
desktop app-target tools, OCR, visual diff, UI-state and UI-element detection,
plugin discovery/execution, semantic desktop graph ingestion/querying,
workflow generation/refinement/execution, and workflow recording tools.
`list_windows` accepts the same filtering and diagnostics fields as the daemon
CLI: `id`, `app`, `title`, `title_regex`, `focused`, `limit`, `sort`,
`backend`, and `diagnose`.

For local inspection without an MCP client:

```bash
peekaboox-agent --version
peekaboox-agent plugins --path examples/plugins
peekaboox-agent windows
peekaboox-agent windows --focused --limit 1 --sort focused
peekaboox-agent windows --app calculator --diagnose
peekaboox-agent desktop-state
```

## Safety Notes

By default, daemon-routed real input injection is denied. Use
`peekabooxd run --profile operator`, `--allow-input`, or
`PEEKABOOX_ALLOW_INPUT=1` only for trusted local automation sessions. Audit logs
are written as JSONL; see [docs/security.md](security.md).

Use `peekabooxd run --sandbox basic` for in-process Linux hardening, or install
`integrations/systemd/peekabooxd-hardened.service` for a stricter observe-only
systemd sandbox.

`peekabooxd` also starts a best-effort `CTRL + ALT + ESC` emergency hotkey
listener. When readable Linux input devices are available, the hotkey shuts the
daemon down and releases common modifier keys. Use `--no-emergency-hotkey` or
`PEEKABOOX_EMERGENCY_HOTKEY=0` in environments where `/dev/input/event*` access
is not available or not desired.
