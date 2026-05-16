# API Contract

The initial API namespace is `peekaboox.v1`.

Core RPCs:

- `CaptureScreen`
- `CaptureDelta`
- `CaptureBackends`
- `MoveMouse`
- `Click`
- `Drag`
- `TypeText`
- `Hotkey`
- `FindElement`
- `ListWindows` with optional `id`, `app`, `title`, `title_regex`, `focused`,
  `limit`, `sort`, `backend`, and `diagnose` fields plus backend metadata in
  the response
- `GetDesktopState`
- `OcrScreen`
- `CompareImages`
- `DetectUiState`
- `DetectUiElements`

Rust and Python implementations must treat the protobuf contract as the stable
boundary between daemon, CLI, MCP server, and agent runtime.

## gRPC API

`peekabooxd` exposes the protobuf contract over gRPC using the service defined
in `proto/peekaboox/v1/peekaboox.proto`.

Incremental capture uses `peekaboox-capture`'s frame path. Backends that can
write image bytes to stdout are decoded directly in memory; file-only backends
remain available as an internal fallback.
The capture crate also exposes `zero_copy_capture_capabilities()`,
`select_zero_copy_backend()`, and `open_pipewire_screencast()` for the optional
Portal/PipeWire DMA-BUF path. With the `pipewire-backend` crate feature,
`capture_screen_dmabuf()` consumes that PipeWire stream and returns DMA-BUF
plane descriptors. `DmaBufFrameDescriptor` owns its duplicated plane file
descriptors and closes them when dropped. `prepare_dmabuf_import_descriptor()` and
`import_dmabuf_frame()` validate those descriptors and produce the backend
handoff contract for EGL, Vulkan, or compute importers. With the optional
`egl-backend` feature, `EglDmaBufImporter` imports the checked descriptor into a
native `EGLImage`, and `EglTextureDmaBufImporter` binds that image as a GLES
`GL_TEXTURE_2D`. The CLI command `peekaboox capture-backends` prints the same
diagnostics as text or JSON, can include missing backend reasons with
`--diagnose`, and can run file, frame, region, or DMA-BUF probes with `--probe`
so target systems can be checked before enabling the importer. `peekaboox
capture-dmabuf --import egl-texture` runs a live
descriptor/import/texture probe when the CLI is built with
`--features pipewire-backend,egl-backend`.

Default gRPC address:

```bash
127.0.0.1:47777
```

Start the daemon with both gRPC and local JSON IPC enabled:

```bash
cargo run -q -p peekabooxd -- run
```

Use a custom gRPC address or disable gRPC:

```bash
cargo run -q -p peekabooxd -- run --grpc-addr 127.0.0.1:47778
cargo run -q -p peekabooxd -- run --no-grpc
```

The daemon starts a best-effort `CTRL + ALT + ESC` emergency hotkey listener by
default. It reads Linux `/dev/input/event*` devices and shuts the daemon down
while releasing common modifiers when the hotkey is pressed. Disable it with:

```bash
cargo run -q -p peekabooxd -- run --no-emergency-hotkey
```

Vision fallback for semantic element and click lookups is disabled by default.
Enable it daemon-wide with:

```bash
cargo run -q -p peekabooxd -- run --vision-fallback
```

The daemon caches the AT-SPI semantic tree briefly so repeated semantic
queries avoid a full desktop traversal. The default TTL is 500ms:

```bash
cargo run -q -p peekabooxd -- run --accessibility-cache-ttl-ms 500
```

It also subscribes to AT-SPI focus, window, and object events and invalidates
the cache when UI state changes. Disable that best-effort listener for debugging
with:

```bash
cargo run -q -p peekabooxd -- run --no-accessibility-events
```

Current gRPC method coverage:

- `CaptureScreen` for full-screen, region, or `window_id` PNG capture
- `CaptureDelta` for persistent low-bandwidth full-screen, region, or
  `window_id` deltas with raw changed-rectangle patch bytes, plus explicit
  full-frame mode
- `CaptureBackends` for screenshot backend discovery plus optional file, frame,
  region, or DMA-BUF probe diagnostics
- `MoveMouse` for absolute pointer movement
- `Click` for coordinate clicks and AT-SPI `semantic_selector` clicks, with
  optional `vision_fallback`
- `Drag` for coordinate drags with button and duration controls
- `TypeText`
- `PasteText` for clipboard-backed text insertion with optional textual
  clipboard restoration
- `Hotkey` for keyboard shortcuts such as `ctrl+s`
- `FindElement` through AT-SPI selector queries, with optional `vision_fallback`
- `ListWindows`
- `GetDesktopState` with windows, active-window metadata, and AT-SPI UI elements
- `OcrScreen` for Tesseract-backed full-screen or region OCR
- `CompareImages` for image-byte visual diffs with region and tolerance options
- `DetectUiState` for image-sequence stable/loading/changing classification
- `DetectUiElements` for vision-only UI-region detection from image bytes
- `ProbeDmaBuf` for the optional DMA-BUF capture/import path
- `ListPlugins` and `CallPluginTool` for plugin discovery and bounded process
  tool execution
- `DesktopFocus`, `DesktopLocate`, `DesktopClick`, `DesktopDrag`,
  `DesktopTypeInto`, and `DesktopAssert` for named app-target desktop helpers
  backed by the Rust desktop profiles. The helpers accept optional
  `window_title` or `window_id` scoping; mutating helper actions also accept
  `verify` to run a post-action guard before returning.

Supported `FindElement` selector forms:

- `Submit` matches element labels containing `Submit`
- `label=Submit`
- `label-exact=Submit`
- `label-regex=^Sub.*`
- `text=Submit`
- `role=push button,label=Submit`
- `role-exact=push button`
- `id=button-1`
- `state=enabled`
- `not-state=disabled`
- `bounds=10,20,90,30`
- `contains=55,35`
- `within=0,0,400,300`
- `intersects=40,40,80,30`
- `min-width=40`
- `min-height=20`
- `confidence>=0.9`

Selector parsing is strict for daemon, CLI, and Rust accessibility lookups:
unknown keys, malformed geometry, invalid numbers, and invalid regexes are
reported as errors. `FindElementRequest` also accepts `app`, `window_title`,
and `window_id` scope fields plus optional vision fallback tuning fields:
`vision_region`, `vision_edge_threshold`, `vision_min_width`,
`vision_min_height`, `vision_min_component_pixels`, `vision_max_elements`, and
`vision_merge_distance`.

`UiElement` responses include `id`, `role`, optional `label`, `bounds`,
optional `center`, `confidence`, AT-SPI `states`, and hierarchy metadata when
available: `window_id`, `window_title`, `app_id`, `parent_id`, and `child_ids`.
`FindElementResponse` additionally reports backend name/kind, warnings, cache
hit status, cache age, and whether vision fallback was used.

When `FindElementRequest.vision_fallback` or `ClickRequest.vision_fallback` is
true, the daemon tries the AT-SPI path first and only captures the current screen
for vision-based fallback if accessibility lookup fails or finds no matching
element. The same fallback can be enabled for all daemon requests with
`--vision-fallback` or `PEEKABOOX_VISION_FALLBACK=1`.

Daemon-routed `MoveMouse`, `Click`, `Drag`, `TypeText`, and `Hotkey` requests
require the daemon to be started with `--profile operator`, `--allow-input`, or
`PEEKABOOX_ALLOW_INPUT=1`.

## Python Runtime Client

The Python package includes generated protobuf bindings under `peekaboox.v1`
and a synchronous runtime client:

```python
from peekaboox.agent import AgentRuntime
from peekaboox.client import Rect
from peekaboox.security import (
    CapabilityProfile,
    ConfirmationPolicy,
    DangerousAction,
)

runtime = AgentRuntime.connect(
    "127.0.0.1:47777",
    capability_profile=CapabilityProfile.ASSIST,
    confirmation_policy=ConfirmationPolicy.require_for([DangerousAction.CLICK]),
    audit_log_path="peekaboox-runtime-audit.jsonl",
)
windows = runtime.list_windows()
focused = runtime.list_windows(focused=True, limit=1, sort="focused")
window_result = runtime.list_windows_result(app="calculator", diagnose=True)
state = runtime.get_desktop_state()
doctor = runtime.doctor()
buttons = runtime.find_element("role=push button", vision_fallback=True)
text = runtime.ocr_region(Rect(x=10, y=20, width=400, height=120), language="eng")
delta = runtime.capture_delta(
    stream_id="agent-loop",
    region=Rect(x=10, y=20, width=400, height=240),
    per_channel_threshold=2,
    low_bandwidth=True,
)
backends = runtime.capture_backends(output="screen.png", diagnose=True, probe="frame")
window_capture = runtime.capture_screen(window_id="window-1")
diff = runtime.compare_image_files("before.png", "after.png", max_changed_ratio=0.01)
ui_state = runtime.detect_ui_state_from_image_files(["frame1.png", "frame2.png", "frame3.png"])
target = runtime.desktop_locate("telegram", "search-input")
runtime.desktop_click("telegram", "search-input", dry_run=True)
runtime.desktop_type_into("telegram", "message-input", "PeekabooX", dry_run=True)
runtime.click_selector("role=push button,label=Submit", vision_fallback=True)
runtime.move_mouse(100, 200)
runtime.drag(100, 200, 360, 260, button="left", duration_ms=350)
runtime.hotkey(["ctrl", "s"])
```

Use `client.find_elements(...)` when callers need backend, warning, cache, and
vision fallback metadata; `client.find_element(...)` remains the backward
compatible tuple-of-elements helper.

`AgentRuntime` accepts either a custom `CapabilityPolicy` or a
`capability_profile` for granular in-process permission checks. The profile
names are `observe`, `plan`, `assist`, and `operator`. The supported
capabilities are `observe`, `click`, `type_text`, `workflow_execute`,
`workflow_record`, `workflow_generate`, `vision`, `memory_read`, and
`memory_write`, `plugin_read`, and `plugin_execute`. Denied calls raise
`CapabilityDeniedError`, and all checks append an in-memory
`CapabilityAuditEvent` retrievable through `runtime.capability_audit()`. This
policy is layered above the daemon's input policy; daemon-routed input still
requires `peekabooxd run --profile operator`, `--allow-input`, or
`PEEKABOOX_ALLOW_INPUT=1`.

`ConfirmationPolicy` adds optional confirmation checks for dangerous runtime
operations before execution. The dangerous action names are `click`,
`type_text`, and `workflow_execute`; `paste_text` is confirmed under
`type_text`, while pointer movement, drags, and hotkeys are confirmed under the
`click` action gate. Required confirmations without a
configured confirmer raise `ConfirmationRequiredError`; rejected confirmations
raise `ConfirmationDeniedError`. Audit events are available through
`runtime.confirmation_audit()`.
When `audit_log_path` is set, capability and confirmation checks are also
persisted as newline-delimited JSON records.

### Semantic Desktop Graph

`AgentRuntime` owns a `MemoryStore` with a `SemanticDesktopGraph`. Call
`ingest_desktop_snapshot()` to sample `GetDesktopState` and store a graph
snapshot, or pass an existing `DesktopState` to avoid an extra daemon call:

```python
snapshot = runtime.ingest_desktop_snapshot()
latest = runtime.latest_desktop_snapshot()
matches = runtime.query_desktop_graph(
    kind="element",
    label_contains="submit",
    contained_by="window-1",
)
graph_json = runtime.memory.export_desktop_graph()
```

Event updates are handled through `record_desktop_event()`. Passing no fresh
`DesktopState` invalidates the current graph and records affected node IDs when
the event targets a known window or element. Passing `state=...` writes a new
snapshot immediately. Queries can opt into automatic refresh:

```python
runtime.record_desktop_event(
    kind="accessibility.element.changed",
    source="accessibility",
    target_id="button-1",
)
status = runtime.desktop_graph_status()
matches = runtime.query_desktop_graph(kind="element", refresh_if_stale=True)
```

Snapshots contain graph nodes for the snapshot, windows, and UI elements, plus
edges for `has_window`, `active_window`, `focused_window`, `has_element`, and
window-to-element `contains` relationships. Graph queries can filter by node
`kind`, label substring, role, exact attribute values, and containment window.
The runtime also uses fresh graph snapshots as a semantic lookup cache:
`find_element(selector)` and semantic `click_selector(selector)` first evaluate
the selector against cached graph elements. If the graph is stale, absent, or no
cached element matches, the call falls back to the daemon path with the same
`vision_fallback` behavior as before.

For persistent memory, pass a SQLite path when connecting or instantiate a
store directly:

```python
from peekaboox.memory import SQLiteMemoryStore

runtime = AgentRuntime.connect(memory_path="peekaboox-memory.sqlite3")
store = SQLiteMemoryStore("peekaboox-memory.sqlite3")
```

`SQLiteMemoryStore` persists key/value memory plus graph snapshots into
normalized `desktop_graph_snapshots`, `desktop_graph_nodes`, and
`desktop_graph_edges` tables. It also persists `desktop_state_events`,
`desktop_graph_invalidations`, and stale/fresh metadata while still supporting
JSON round-trips through `export_desktop_graph()` and `import_desktop_graph()`.

`AgentRuntime` also exposes deterministic workflow execution:

- `plan_workflow(goal)` creates the current simple observe workflow.
- `generate_workflow(goal, refresh_desktop_graph=False)` creates an editable
  workflow draft from a goal and optional graph context.
- `save_generated_workflow(goal, path, format_name=None)` writes that draft as
  JSON or YAML.
- `refine_workflow(goal, workflow=None, refresh_desktop_graph=False)` sends a
  draft through the configured structured workflow provider.
- `replan_workflow(goal, failed_workflow, failed_result, ...)` asks the
  configured replanning provider for a validated replacement workflow after a
  failed execution.
- `save_refined_workflow(goal, path, workflow=None, format_name=None)` writes
  the refined draft as JSON or YAML.
- `execute_goal(goal, replan_on_failure=True, max_replans=1)` plans and
  executes that workflow, then can run one or more validated replans after a
  failure.
- `execute_workflow(workflow)` executes explicit `WorkflowStep` sequences.
- `load_workflow_file(path)` loads a JSON or YAML workflow definition.
- `execute_workflow_file(path)` loads and executes that workflow definition.
- `start_recording(name)` starts capturing subsequent actions as workflow steps.
- `stop_recording()` ends the active recording and returns a `Workflow`.
- `recorded_workflow()` returns the active or last completed recording.
- `save_recording(path, format_name=None)` exports the recording as JSON/YAML.
- `execute_step(step)` retries a single step up to `retries + 1` attempts.

Execution results include per-attempt messages, verification results, and
structured recovery metadata. Failed workflows report `failed_step`, `action`,
`reason`, `attempts`, and `next_action`. Selector replay can also report
`strategies` and `events` when self-healing was attempted. Built-in actions are
`observe`, `find_element`, `click`, `move_mouse`, `drag`, `type_text`,
`paste_text`, `hotkey`, `list_windows`, and `get_desktop_state`.
`click`, `type_text`, `paste_text`, and other input actions sample desktop state after successful execution as the
current verification hook; callers can pass a custom verifier for stricter
domain-specific checks.
For selector-based `find_element` and `click` steps, retry attempts first
refresh the semantic desktop graph and later enable `vision_fallback` when the
original step did not request it. Each `ActionAttempt` exposes a `recovery`
object for the strategy applied before that attempt.
Generated workflows are drafts, not implicit execution. They start with
`observe`, use graph-backed selectors when a target label is known, include
`find_element` before selector clicks, and can be serialized through the same
JSON/YAML workflow file helpers.
Provider-backed refinement and replanning are optional and never execute
provider output directly. A provider must return a `Workflow`, a workflow
object, or JSON/YAML workflow text. The runtime validates every returned
`WorkflowStep` against the supported action set before exposing, saving, or
executing it.

Workflow files use the same fields as `WorkflowStep`: `action`, `selector`,
`value`, `x`, `y`, `vision_fallback`, and `verify`. JSON is parsed with the
standard library; YAML support covers the repository workflow shape without an
extra runtime dependency. The recorder writes the same schema, so recorded
workflows can be reviewed, edited, and replayed through `execute_workflow_file`.
For recorded coordinate clicks, the runtime attempts to resolve the point
against a fresh semantic desktop graph and records a unique semantic selector
when possible. If no element resolves, the step falls back to `x`/`y`.

## MCP Tool Surface

`peekaboox-mcp` exposes a local MCP-style registry with tool descriptors,
JSON-schema input metadata, and a `call_tool(name, arguments)` dispatcher bound
to `AgentRuntime`. It also includes a stdio JSON-RPC transport for MCP clients:

```bash
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --list-tools
PYTHONPATH=python/src python3 -m peekaboox.mcp.server
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --audit-log runtime-audit.jsonl
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --capability-profile observe
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --preflight-mode strict
```

The stdio transport handles `initialize`, `ping`, `tools/list`, `tools/call`,
and `notifications/initialized`. Tool calls return `structuredContent` plus a
serialized JSON text content block for compatibility.
Tool execution requires the Python runtime dependencies and a reachable
PeekabooX daemon at `PEEKABOOX_GRPC_TARGET` or the `--target` address; without
those dependencies the server can still list tool descriptors for inspection.
MCP tools are bound to the runtime's `CapabilityPolicy`; denied tool execution
returns a normal `tools/call` result with `isError: true` and
`structuredContent.error` set to `CapabilityDeniedError`.
When the runtime has a `ConfirmationPolicy`, missing or denied confirmations are
reported the same way with `ConfirmationRequiredError` or
`ConfirmationDeniedError`.
Set `--audit-log` or `PEEKABOOX_RUNTIME_AUDIT_LOG` to persist those runtime
checks for MCP sessions. Set `--capability-profile`,
`PEEKABOOX_MCP_CAPABILITY_PROFILE`, or `PEEKABOOX_CAPABILITY_PROFILE` to apply a
reusable runtime allowlist to MCP tool calls.
Set `--preflight-mode off|warn|strict` and `--preflight-timeout <seconds>` to
enable Doctor-backed preflight checks directly at MCP server startup.

The current tool surface includes:

- `capture_screen`
- `capture_delta`
- `capture_backends`
- `doctor`
- `preflight`
- `probe_dmabuf`
- `click`
- `type_text`
- `paste_text`
- `find_element`
- `list_windows`
- `list_plugins`
- `call_plugin_tool`
- `get_desktop_state`
- `desktop_focus`
- `desktop_locate`
- `desktop_click`
- `desktop_drag`
- `desktop_type_into`
- `desktop_assert`
- `ingest_desktop_snapshot`
- `latest_desktop_snapshot`
- `record_desktop_event`
- `desktop_graph_status`
- `refresh_desktop_graph`
- `query_desktop_graph`
- `ocr_screen`
- `compare_images`
- `detect_ui_state`
- `detect_ui_elements`
- `execute_goal`
- `generate_workflow`
- `save_generated_workflow`
- `refine_workflow`
- `save_refined_workflow`
- `execute_workflow`
- `execute_workflow_file`
- `start_workflow_recording`
- `stop_workflow_recording`
- `get_recorded_workflow`
- `save_recorded_workflow`

`click` accepts either `x`/`y` coordinates or `selector`/`semantic_selector`.
`list_windows` supports `id`, `app`, `title`, `title_regex`, `focused`,
`limit`, `sort`, `backend`, and `diagnose` arguments through MCP, matching the
daemon CLI and Python runtime client.
`capture_screen` accepts optional `region` or `window_id`; `capture_delta`
accepts `stream_id`, `reset`, optional `region` or `window_id`,
`per_channel_threshold`, and `low_bandwidth` for persistent low-bandwidth
capture streams. `capture_backends` accepts `output`, optional `region`,
`diagnose`, and `probe` values `none`, `file`, `frame`, `region`, `dmabuf`, or
`all`. `doctor` accepts optional `strict` and `timeout_seconds` arguments and
returns the structured `peekaboox doctor --json` health checks with per-check
`category`/`severity` fields and top-level category summaries.
`preflight` accepts `categories`, optional `operation`, `refresh`,
`timeout_seconds`, and `require`; it returns the Doctor-backed category gate
used by `AgentRuntime(preflight_mode="strict")` before live automation.
The desktop helper tools accept supported app profile names such as `telegram`,
`paint`, `drawing`, `pinta`, `kolourpaint`, and `text-editor`, plus named
targets such as Telegram's `search-input`/`message-input`, Paint's `canvas`, or
Text Editor's `document`. Use `window_id` for exact-window targeting when
multiple windows share an app profile; use `verify: true` on focus, click, drag,
or type-into calls when the caller needs an immediate postcondition check.
`click` and `find_element` both accept `vision_fallback: true`; when a fresh
graph cache hits, `find_element` returns cached elements directly and semantic
`click` uses the cached element center as a coordinate click.
File-oriented vision tools use `expected_path`/`actual_path` or `image_path`
arguments so MCP callers do not need to pass raw image bytes.
`ingest_desktop_snapshot` samples the current desktop state and appends it to
the runtime's semantic desktop graph; `latest_desktop_snapshot` returns the most
recent stored snapshot or `null`. `record_desktop_event` records a desktop
change and invalidates the graph; `desktop_graph_status` reports stale state and
the latest invalidation; `refresh_desktop_graph` samples a fresh graph snapshot.
`query_desktop_graph` filters stored graph nodes by `kind`, `label_contains`,
`role`, `attribute_equals`, `contained_by`, `latest_only`, and optionally
`refresh_if_stale`.
`execute_goal` accepts a `goal` string plus optional `replan_on_failure` and
`max_replans`, then runs the runtime planner plus workflow loop.
`generate_workflow` accepts `goal`, optional
`refresh_desktop_graph`, and optional `format` of `json` or `yaml`; it returns
the workflow object plus serialized text. `save_generated_workflow` writes the
same draft to `path`. `refine_workflow` accepts `goal`, optional `workflow`,
optional `refresh_desktop_graph`, and optional `format`; it returns the validated
provider-refined workflow object plus serialized text. `save_refined_workflow`
writes that validated draft to `path`. `execute_workflow` accepts `name` and
`steps`; each step supports `action`, `selector`, `value`, `x`, `y`,
`from_x`, `from_y`, `to_x`, `to_y`, `button`, `duration_ms`,
`vision_fallback`, and `verify`. `execute_workflow_file` accepts `path` and
loads a JSON/YAML workflow file before executing the same retry/verification
loop. Workflow recording tools capture subsequent `capture_screen`,
`find_element`, `click`, `type_text`, and `paste_text` calls as replayable steps;
`save_recorded_workflow` accepts `path` and an optional `format` of `json` or
`yaml`.
`list_plugins` accepts optional `paths`, and `call_plugin_tool` accepts
`plugin_id`, `tool`, optional `arguments`, optional `paths`, and optional
`timeout_seconds` and `max_output_bytes`; execution is gated by
`plugin_execute`.
Recorded coordinate clicks are enriched from the graph cache when possible, so
the saved step can replay through `selector` instead of fixed `x`/`y`.
Workflow tool results include the same per-attempt verification and recovery
metadata as the Python runtime API, including selector self-healing strategies
such as `refresh_desktop_graph` and `vision_fallback`.

The client uses the checked-in generated modules from
`proto/peekaboox/v1/peekaboox.proto`. Regenerate them after proto changes with:

```bash
python3 -m pip install -e "python[dev]"
python3 -m grpc_tools.protoc \
  -I proto \
  --python_out=python/src \
  --grpc_python_out=python/src \
  proto/peekaboox/v1/peekaboox.proto
```

## Local IPC

For the Rust CLI, `peekabooxd` also exposes the daemon surface over a local Unix
socket using newline-delimited JSON.

Default socket:

```bash
$XDG_RUNTIME_DIR/peekabooxd.sock
```

Start the daemon:

```bash
cargo run -q -p peekabooxd -- run
cargo run -q -p peekabooxd -- run --sandbox basic
```

Route CLI commands through the daemon:

```bash
cargo run -q -p peekaboox-cli -- --daemon windows
cargo run -q -p peekaboox-cli -- --daemon windows --focused --limit 1 --json
```

Inspect the installed Python runtime without starting an MCP client:

```bash
peekaboox-agent --version
peekaboox-agent plugins --path examples/plugins
peekaboox-agent windows
peekaboox-agent --preflight-mode strict windows
peekaboox-agent windows --focused --limit 1 --sort focused
peekaboox-agent windows --app calculator --diagnose
peekaboox-agent desktop-state
```

Supported request methods:

- `ping`
- `capture`
- `capture_delta`
- `capture_backends`
- `desktop_focus`
- `desktop_locate`
- `desktop_click`
- `desktop_drag`
- `desktop_type_into`
- `desktop_assert`
- `click`
- `move_mouse`
- `drag`
- `type_text`
- `paste_text`
- `hotkey`
- `list_windows`
- `find_elements`
- `ocr`
- `compare_images`
- `detect_ui_state`
- `detect_ui_elements`
- `probe_dmabuf`
- `list_plugins`
- `call_plugin_tool`

Daemon-routed `capture` requests accept `output` plus optional `region` or
`window_id`. Daemon-routed `capture_delta` requests accept `stream_id`, `reset`,
optional `region` or `window_id`, `per_channel_threshold`, and `low_bandwidth`.
`low_bandwidth=true` is the default and returns changed-rectangle patches after
the first frame; `low_bandwidth=false` forces a full-frame patch for that
request. Responses carry patch bytes as `patch_base64` and echo `low_bandwidth`.
Daemon-routed `capture_backends` requests accept `output`, optional `region`,
`diagnose`, and `probe` values `none`, `file`, `frame`, `region`, `dmabuf`, or
`all`, returning the same backend diagnostics and probe results as the CLI.
Daemon-routed desktop helper requests accept the same `window_id`,
`window_title`, and `verify` fields as the gRPC/Python/MCP surfaces.
Daemon-routed `probe_dmabuf` requests accept `import_target` values `compute`,
`egl`, or `egl_texture` when `peekabooxd` is built with the matching
`pipewire-backend`/`egl-backend` features.
Daemon-routed `list_plugins` requests accept optional plugin `paths`; otherwise
the daemon uses its configured `--plugin-path` values plus SDK defaults.
Daemon-routed `call_plugin_tool` requests accept `plugin_id`, `tool`, optional
JSON `arguments`, optional plugin `paths`, `timeout_ms`, and
`max_output_bytes`. Arguments are validated against the tool `input_schema`, the
process runs with a restricted environment, and stdout/stderr are capped.
Daemon-routed `find_elements` requests accept `vision_fallback: true`,
`app`, `window_title`, `window_id`, `vision_region`, `vision_edge_threshold`,
`vision_min_width`, `vision_min_height`, `vision_min_component_pixels`,
`vision_max_elements`, and `vision_merge_distance`.
Daemon-routed `click`, `type_text`, `paste_text`, pointer movement, drags, and
hotkeys require `dry_run: true` where supported unless the daemon was started
with `--profile operator`, `--allow-input`, or `PEEKABOOX_ALLOW_INPUT=1`.
Use `--sandbox basic` for `no_new_privileges` and non-dumpable daemon process
state. Use `--sandbox strict` only on Linux hosts where unprivileged user
namespaces are available; startup fails if namespace isolation cannot be
applied.

The Rust CLI accepts both coordinate and semantic click targets:

```bash
cargo run -q -p peekaboox-cli -- click --x 100 --y 200 --dry-run
cargo run -q -p peekaboox-cli -- click --text "Submit" --dry-run
cargo run -q -p peekaboox-cli -- click --selector "role=push button,label=Submit" --dry-run
cargo run -q -p peekaboox-cli -- click --selector "role=button" --vision-fallback --dry-run
cargo run -q -p peekaboox-cli -- elements --selector "role=button" --vision-fallback
```

OCR through the CLI:

```bash
cargo run -q -p peekaboox-cli -- ocr
cargo run -q -p peekaboox-cli -- ocr --region 10,20,400,120 --language eng
cargo run -q -p peekaboox-cli -- --daemon ocr --language eng
```

Visual comparison through the CLI:

```bash
cargo run -q -p peekaboox-cli -- compare before.png after.png
cargo run -q -p peekaboox-cli -- compare --expected before.png --actual after.png --region 10,20,400,120 --threshold 3 --max-changed-ratio 0.01
cargo run -q -p peekaboox-cli -- --daemon compare before.png after.png
```

UI-state/loading detection through the CLI:

```bash
cargo run -q -p peekaboox-cli -- state frame1.png frame2.png frame3.png
cargo run -q -p peekaboox-cli -- state --image frame1.png --image frame2.png --threshold 3 --stable-max-changed-ratio 0.001 --loading-min-changed-ratio 0.02
cargo run -q -p peekaboox-cli -- --daemon state frame1.png frame2.png
```

Vision-only UI element detection through the CLI:

```bash
cargo run -q -p peekaboox-cli -- vision-elements screenshot.png
cargo run -q -p peekaboox-cli -- vision-elements --image screenshot.png --region 10,20,400,300 --threshold 24 --min-width 8 --max-elements 25
cargo run -q -p peekaboox-cli -- --daemon vision-elements screenshot.png
```

## Rust Vision OCR

`peekaboox-vision` exposes the current OCR surface:

- `OcrBackend` for provider implementations
- `TesseractOcrBackend` using the `tesseract` CLI and TSV output
- `ocr_screen()` for full-screen OCR
- `ocr_region(Rect)` for region-filtered OCR
- `ocr_image_file(path, region)` for OCR over an existing image file
- daemon/gRPC, Python, MCP, and CLI bindings for full-screen, region,
  image-file, and window-scoped OCR
- Tesseract controls for language, page segmentation mode, engine mode, DPI,
  minimum confidence, character whitelist, and repeated `key=value` config
  entries
- preprocessing controls for scale, grayscale, threshold, invert, contrast,
  and deskew

OCR text is returned as line blocks and word blocks with `UiElement` metadata
carrying role, label, bounds, confidence, and empty states. Region OCR crops
before Tesseract runs, then maps coordinates back to the screen or source image
coordinate space.

## Rust Vision Comparison

`peekaboox-vision` also exposes a frame-based visual comparison foundation:

- `VisualCompareOptions` selects an optional `Rect` region, per-channel pixel
  threshold, and maximum allowed changed-pixel ratio.
- `compare_frames(expected, actual, options)` compares `CaptureFrame` values in
  RGB space across `Rgb8`, `Rgba8`, and `Bgra8`.
- `VisualDiffResult` reports compared pixels, changed pixels, changed ratio,
  mean absolute error, maximum channel delta, changed bounds, and whether the
  frames match the requested tolerance.
- `incremental_capture_delta(previous, current, sequence, options)` emits an
  initial full-frame patch when no previous frame exists, otherwise emits only a
  densely packed patch for the changed bounds reported by `compare_frames`.
- `IncrementalCaptureDelta` carries sequence number, source frame dimensions,
  pixel format, full-frame marker, changed bounds, changed-pixel statistics,
  patch stride, and patch bytes. Unchanged frames return no bounds and empty
  patch bytes.
- `CaptureDelta` exposes this over gRPC using daemon-held stream state. The
  daemon keeps the previous decoded frame per `stream_id`; `reset=true` forces a
  fresh full-frame patch. `low_bandwidth=false` also suppresses previous-frame
  reuse for that request and returns a full-frame patch while preserving the
  stream sequence. Region targets use native region capture, and `window_id`
  targets resolve current window bounds through the window enumerator before
  capture. Local IPC and CLI expose the same stream controls, and Python/MCP
  clients return patch bytes as bytes/base64 respectively.
- The live daemon path calls `peekaboox-capture::capture_screen_frame()` so
  incremental capture no longer performs PNG temp-file decoding in daemon code;
  stdout-capable capture tools feed decoded frames directly, with file-only
  capture retained as a backend-level fallback.
- `peekaboox-capture` exposes DMA-BUF zero-copy capability probing for
  Portal/PipeWire Wayland sessions and can open a ScreenCast session through
  `CreateSession`, `SelectSources`, `Start`, and `OpenPipeWireRemote`. When the
  crate is built with `--features pipewire-backend`, `capture_screen_dmabuf()`
  consumes the opened PipeWire stream, negotiates DMA-BUF buffers, and returns
  `DmaBufFrameDescriptor` plane metadata. The capture crate also exposes
  `DmaBufImportTarget`, `DmaBufFrameImportDescriptor`, and
  `ValidatingDmaBufImporter` so graphics/compute backends receive a checked
  DMA-BUF handoff before creating native EGL/Vulkan resources. With
  `--features egl-backend`, `EglDmaBufImporter` opens an EGL display, checks
  `EGL_EXT_image_dma_buf_import`, builds `EGL_LINUX_DMA_BUF_EXT` attributes, and
  destroys the resulting `EGLImage` when the imported frame is dropped.
  `EglTextureDmaBufImporter` additionally creates a GLES2 context, requires
  `GL_OES_EGL_image`, binds the image to `GL_TEXTURE_2D`, and deletes the texture
  on drop. The CLI can run the same compute/EGL/GLES texture probe through the
  daemon over local IPC with `peekaboox --daemon capture-dmabuf --import ...`.
  The default build keeps the stable frame path on owned CPU bytes and reports
  that optional backend features are disabled.
- `CompareImages` exposes the same diff result over gRPC using image bytes.
- The local daemon IPC and CLI compare image file paths and use the same
  tolerance fields.

Small image fixtures live under `tests/fixtures/vision` for regression tests,
including screen-like PBM fixtures for decoder-backed UI-element detection,
loading-state transitions, and vision-fallback lookup behavior.

## Plugin SDK

Plugin discovery is defined by `docs/plugins.md`. The stable manifest name is
`peekaboox.plugin.json`, and the current schema version is
`peekaboox.plugin.v1`. The SDK validates plugin ids, declared capabilities,
process entrypoints, and JSON-schema-shaped tool input metadata. The same
validated descriptors are exposed through `peekaboox plugins`, daemon JSON IPC
and gRPC `list_plugins`, Python `AgentRuntime.list_plugins()`, and MCP
`list_plugins`. Declared process-plugin tools can be executed through
`peekaboox plugin-call`, daemon JSON IPC and gRPC `call_plugin_tool`, Python
`AgentRuntime.call_plugin_tool()`, and MCP `call_plugin_tool`, gated by the
`plugin_execute` capability where the Python runtime is in the path.

## Rust UI-State Detection

`peekaboox-vision` includes a deterministic UI-state detection foundation built
on the same frame comparison primitives:

- `UiStateOptions` selects an optional `Rect` region, per-channel threshold,
  stable changed-pixel ratio, loading changed-pixel ratio, and required trailing
  stable transitions.
- `detect_ui_state(frames, options)` compares adjacent `CaptureFrame` samples
  and classifies the sequence as `Stable`, `Loading`, or `Changing`.
- `UiStateResult` reports transition counts, trailing stability, latest diff,
  maximum and mean changed ratio, and aggregate changed bounds.
- `detect_ui_state_from_image_files` and `detect_ui_state_from_image_bytes`
  provide decoder-backed helpers for fixtures and API bindings.
- `DetectUiState` exposes the same result over gRPC using repeated image bytes.
- The local daemon IPC and CLI compare image file sequences with the same
  tolerance fields.

## Rust UI Element Detection

`peekaboox-vision` includes a first deterministic UI-element detection
foundation for accessibility fallback scenarios:

- `UiElementDetectionOptions` selects an optional `Rect` region, edge/contrast
  threshold, minimum component size, maximum result count, and merge distance.
- `detect_ui_elements(frame, options)` finds salient visual components and
  returns them as `UiElement` values with role `visual-region`, bounds,
  confidence, and visible state.
- `detect_ui_elements_from_image_file` and `detect_ui_elements_from_image_bytes`
  provide decoder-backed helpers for fixtures and API bindings.
- `HeuristicVisionBackend` implements `VisionBackend::detect_ui_elements` with
  this fallback detector while frame OCR remains delegated to the OCR pipeline.
- `DetectUiElements` exposes the same detector over gRPC using image bytes.
- The local daemon IPC and CLI detect elements from image file paths and return
  the same `UiElement` shape used by accessibility queries.
