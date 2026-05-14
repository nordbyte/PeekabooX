# PeekabooX

PeekabooX is a Linux-native AI computer-use and desktop automation platform.

The `v1.0.0` foundation release provides:

- Rust daemon and CLI for local Linux desktop observation and controlled action.
- Python agent runtime, MCP server, workflow helpers, memory store, and plugin SDK bindings.
- Shared protobuf and local JSON IPC contracts.
- Packaging for Rust install, Python wheel, Debian, Docker smoke images, Nix development shells, and release manifests.

## Current Entry Points

```bash
cargo check --workspace
python3 -m compileall python/src
PYTHONPATH=python/src python3 benchmarks/perf_baseline.py --iterations 30
```

Build local install artifacts:

```bash
packaging/install-rust.sh
python3 -m pip wheel --no-deps -w target/python-wheel ./python
python3 packaging/debian/build_deb.py --check
python3 packaging/debian/build_deb.py
python3 packaging/smoke_install.py --skip-cargo-install
python3 packaging/release_manifest.py
docker build --target smoke -t peekaboox:smoke .
```

Create a screenshot in the current Linux desktop session:

```bash
cargo run -q -p peekaboox-cli -- capture --output screenshot.png
```

The current capture implementation detects the session and prefers
`xdg-desktop-portal` on Wayland, with GNOME, wlroots, KDE, and X11 command
fallbacks where available. For incremental capture it first tries direct
stdout-to-frame capture through backends that can emit image bytes without a
daemon-managed screenshot file, then falls back to file-only capture backends.
Use `capture-backends` to inspect the selected image backends and whether the
optional Portal/PipeWire DMA-BUF zero-copy path is available on the current
session. The Rust capture API can also open the Portal ScreenCast session and
PipeWire remote through `open_pipewire_screencast()`. Builds compiled with the
`pipewire-backend` feature add a PipeWire stream consumer that negotiates
DMA-BUF buffers and returns frame/plane descriptors. The same API exposes a
validated DMA-BUF import descriptor for EGL, Vulkan, or compute backends, and
the optional `egl-backend` feature can import that descriptor into a native
`EGLImage` or bind it as a GLES `GL_TEXTURE_2D` without copying through CPU
image bytes. With a matching feature-built daemon running, the CLI can route the
same live probe through local IPC. These feature builds require the native `libpipewire-0.3`,
`libspa-0.2`, `libEGL`, and `libGLESv2` development packages.

```bash
cargo run -q -p peekaboox-cli -- capture-backends
cargo run -q -p peekaboox-cli --features pipewire-backend -- capture-dmabuf
cargo run -q -p peekaboox-cli --features pipewire-backend,egl-backend -- capture-dmabuf --import egl
cargo run -q -p peekaboox-cli --features pipewire-backend,egl-backend -- capture-dmabuf --import egl-texture
cargo run -q -p peekaboox-cli --features pipewire-backend,egl-backend -- --daemon capture-dmabuf --import egl-texture
```

Check or execute basic input automation:

```bash
cargo run -q -p peekaboox-cli -- click --x 100 --y 200 --dry-run
cargo run -q -p peekaboox-cli -- click --text "Submit" --dry-run
cargo run -q -p peekaboox-cli -- move --x 100 --y 200 --dry-run
cargo run -q -p peekaboox-cli -- drag --from 100,200 --to 320,240 --dry-run
cargo run -q -p peekaboox-cli -- type --dry-run "Hello World"
cargo run -q -p peekaboox-cli -- hotkey --dry-run ctrl+s
```

Remove `--dry-run` to perform the action. The current input implementation
prefers direct `/dev/uinput` pointer events on Wayland, uses `ydotool` for
Wayland hotkeys, prefers `wtype` for Wayland text where available with
`ydotool` as fallback, and prefers `xdotool` on X11. Semantic click targets use
AT-SPI and resolve to the center of the matching UI element.

Use the higher-level desktop helper when an action needs app focus, screenshot
layout detection, and guards around state-sensitive targets:

```bash
cargo run -q -p peekaboox-cli -- desktop profiles
cargo run -q -p peekaboox-cli -- desktop focus --app telegram
cargo run -q -p peekaboox-cli -- desktop locate --app telegram --target search-input
cargo run -q -p peekaboox-cli -- desktop type-into --app telegram --target search-input --clear "Saved Messages"
cargo run -q -p peekaboox-cli -- desktop assert --app telegram --target send-button --not-active
cargo run -q -p peekaboox-cli -- desktop drag --app drawing --target canvas --from-ratio 0.2,0.3 --to-ratio 0.8,0.3
cargo run -q -p peekaboox-cli -- desktop type-into --app text-editor --target document --window-title notes.txt --clear "PeekabooX"
```

The built-in profiles include `telegram`, `paint`, `drawing`, `pinta`,
`kolourpaint`, and `text-editor`. They use the safest available path in order:
existing window focus where window enumeration is available, GNOME Overview or
application launch fallback, then app-specific visual layout targets such as
Telegram's `search-input` and `message-input`, Paint's `canvas`, or Text
Editor's `document`. Pass `--window-title <text>` to `desktop focus`, `locate`,
`click`, `drag`, `type-into`, or `assert` when an action must target one
specific window instead of the currently focused matching app.

List visible desktop windows:

```bash
cargo run -q -p peekaboox-cli -- windows
```

The current window enumeration implementation tries GNOME Shell Introspect on
GNOME, falls back to AT-SPI for Wayland-accessible applications, and uses
`xdotool` for X11/XWayland windows.

Run the local daemon API and send CLI commands through it:

```bash
cargo run -q -p peekabooxd -- run
cargo run -q -p peekaboox-cli -- --daemon windows
```

The daemon listens on `127.0.0.1:47777` for gRPC by default using
`proto/peekaboox/v1/peekaboox.proto`. It also listens on
`$XDG_RUNTIME_DIR/peekabooxd.sock` for the CLI's newline-delimited JSON protocol:
`ping`, `capture`, `capture_delta`, `move_mouse`, `click`, `drag`, `type_text`,
`hotkey`, `list_windows`, `find_elements`, `ocr`, `compare_images`, `detect_ui_state`,
`detect_ui_elements`, `probe_dmabuf`, and `list_plugins`.
Daemon-side semantic queries use a short AT-SPI cache with a 500ms default TTL;
override it with `peekabooxd run --accessibility-cache-ttl-ms <ms>`. The daemon
also listens for AT-SPI focus/window/object events to invalidate that cache when
the UI changes. Start it with `--vision-fallback` or set
`PEEKABOOX_VISION_FALLBACK=1` to let semantic element and click requests fall
back to a live screenshot-based detector when accessibility returns no match.

List semantic elements from the CLI:

```bash
peekaboox elements --role "push button" --state enabled --contains 100,200
peekaboox elements --selector "role=push button" --vision-fallback
peekaboox --daemon find --selector "role=push button,label=Submit,confidence>=0.9"
```

The Rust vision crate now has a Tesseract-backed OCR provider abstraction for
full-screen and region OCR. It expects the `tesseract` executable to be
available on `PATH`; set `PEEKABOOX_OCR_LANGUAGE=eng` or another installed
language code to override the default language. OCR is exposed through the
Rust crate, gRPC, Python client, and CLI:

```bash
peekaboox ocr --region 10,20,400,120 --language eng
peekaboox --daemon ocr
```

The vision crate also includes frame-based visual comparison primitives for
region diffing and action verification. They are exposed through the Rust crate,
gRPC, Python client, daemon IPC, and CLI:

```bash
peekaboox compare before.png after.png --max-changed-ratio 0.01
peekaboox --daemon compare --expected before.png --actual after.png --threshold 3
```

The same Rust foundation now exposes `incremental_capture_delta` for
low-bandwidth capture paths: the first sample produces a full-frame patch, later
samples produce only the changed rectangle with packed patch bytes. The daemon
keeps previous-frame state per stream and exposes it over gRPC, local IPC, CLI,
Python runtime, and MCP:

```bash
peekaboox --daemon capture-delta --stream agent-loop --threshold 2
peekaboox --daemon capture-delta --stream agent-loop --low-bandwidth
peekaboox --daemon capture-delta --stream agent-loop --full-frame
peekaboox --daemon capture-delta --stream agent-loop --reset
peekaboox capture-backends
peekaboox plugins --path examples/plugins
```

Plugins use the declarative SDK manifest `peekaboox.plugin.json`. Discovery is
available through the CLI, daemon JSON IPC, Python runtime, and MCP
`list_plugins`; Python and MCP can execute declared process tools through
`call_plugin_tool`. See `docs/plugins.md` and `examples/plugins/system-info`.

Release versioning and artifact manifest generation are documented in
`docs/release.md`. CI uploads wheels, Debian packages, Docker metadata,
`release-manifest.json`, and `SHA256SUMS` for release builds; tag builds publish
the same files as GitHub Release assets.

The Rust vision crate also has the first UI-state detection foundation. It
analyzes adjacent frame diffs to classify a sampled screen sequence as stable,
loading, or changing, with tunable pixel threshold, region, and stability
requirements. It is exposed through the Rust crate, gRPC, Python client, daemon
IPC, and CLI:

```bash
peekaboox state frame1.png frame2.png frame3.png
peekaboox --daemon state --image frame1.png --image frame2.png
```

The Rust vision crate also includes the first vision-only UI element detection
foundation. It groups contrast/edge components into `UiElement` fallback
regions with bounds, confidence, and visible state for cases where
accessibility data is missing. It is exposed through the Rust crate, gRPC,
Python client, daemon IPC, and CLI:

```bash
peekaboox vision-elements screenshot.png --min-width 8
peekaboox --daemon vision-elements --image screenshot.png --max-elements 25
```

Semantic element lookup and semantic click flows can also opt into this detector
as a fallback while keeping AT-SPI as the primary source:

```bash
peekaboox click --selector "role=button" --vision-fallback --dry-run
peekaboox --daemon elements --selector "role=button" --vision-fallback
```

Regression image fixtures are kept in `tests/fixtures/vision`, including
screen-like PBM fixtures for UI-element detection, loading-state checks, and
vision-fallback lookup tests.

Use the Python runtime client against the daemon:

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
print(runtime.find_element("role=push button"))
print(runtime.ocr_screen().text)
print(
    runtime.capture_delta(
        stream_id="agent-loop",
        region=Rect(x=10, y=20, width=400, height=240),
    ).changed_bounds
)
print(runtime.compare_image_files("before.png", "after.png").matches)
print(runtime.detect_ui_state_from_image_files(["frame1.png", "frame2.png"]).state)
print(runtime.detect_ui_elements_from_image_file("screenshot.png").elements)
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
before dangerous `click`, `type_text`, or `execute_workflow` operations. Pointer
movement, drags, and hotkeys use the `click` confirmation gate. Decisions are
available through `runtime.confirmation_audit()`.
Pass `audit_log_path` or run `peekaboox-mcp --audit-log <path>` to persist those
runtime security checks as JSONL.

The runtime also has a deterministic workflow execution loop. `WorkflowStep`
actions such as `find_element`, `click`, `move_mouse`, `drag`, `hotkey`,
`type_text`, and `observe` are retried according to `AgentRuntime.retries`,
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
desktop graph is available, the generator uses it to produce stronger selectors:

```python
runtime.ingest_desktop_snapshot()
draft = runtime.generate_workflow("Click Submit and type 'Hello'")
runtime.save_generated_workflow("Click Submit and type 'Hello'", "generated.yaml")
```

Projects can attach a structured refinement provider to `PlanningEngine`. The
provider may improve a draft, but PeekabooX only accepts returned `Workflow`
objects or JSON/YAML workflow definitions that validate as supported
`WorkflowStep` sequences:

```python
refined = runtime.refine_workflow("Click Submit and type 'Hello'")
runtime.save_refined_workflow("Click Submit and type 'Hello'", "refined.yaml")
```

During replay, selector-based `find_element` and `click` steps self-heal across
retries. After an initial selector failure, the runtime refreshes the semantic
desktop graph; on a later retry it enables `vision_fallback` if the step did not
already request it. Step results report the applied recovery strategies.

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

When recording coordinate clicks, the runtime samples semantic desktop state if
needed and stores a stable selector such as `role=push button,label=Submit`
when the clicked point resolves to a unique element. That lets replay use the
element's current bounds instead of the original click coordinates.

The runtime now keeps a semantic desktop graph in memory. A desktop state
snapshot turns windows, UI elements, and containment relationships into a
queryable graph. Use `SQLiteMemoryStore` or `AgentRuntime.connect(memory_path=...)`
to persist memory values and graph snapshots across runs:

```python
from peekaboox.memory import SQLiteMemoryStore

runtime = AgentRuntime.connect(memory_path="peekaboox-memory.sqlite3")
snapshot = runtime.ingest_desktop_snapshot()
print(snapshot.active_window_id)
print(runtime.query_desktop_graph(kind="element", label_contains="submit", contained_by="window-1"))
graph_json = runtime.memory.export_desktop_graph()
```

Desktop events can now invalidate or refresh that graph. Events without a fresh
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

`peekaboox-mcp` now exposes a concrete MCP-style tool registry and dispatcher
over the Python runtime. Registered tools include `capture_screen`,
`capture_delta`, `click`, `type_text`, `find_element`, `list_windows`,
`get_desktop_state`, OCR, visual diff, UI-state, UI-element detection, plugin
discovery/execution, semantic desktop graph snapshot ingestion, event
invalidation, graph querying, `execute_goal`, `generate_workflow`,
`save_generated_workflow`, `refine_workflow`, `save_refined_workflow`,
`execute_workflow`, and `execute_workflow_file`, plus workflow recording tools
to start, stop, inspect, and save recorded workflows.
`find_element`, semantic `click`, and workflow click/find steps accept
`vision_fallback` so external agents can opt into the same fallback path as the
CLI and gRPC APIs.
Workflow execution results include per-attempt recovery metadata, so MCP
callers can see whether graph refresh or vision fallback healed a replay.

Run it as a stdio MCP server after installing the Python package, or directly
from the checkout during development:

```bash
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --list-tools
PYTHONPATH=python/src python3 -m peekaboox.mcp.server
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --audit-log runtime-audit.jsonl
PYTHONPATH=python/src python3 -m peekaboox.mcp.server --capability-profile observe
```

Tool execution through MCP requires the Python runtime dependencies and a
running `peekabooxd` reachable at `PEEKABOOX_GRPC_TARGET` or `--target`.
For local inspection without an MCP client, `peekaboox-agent --version`,
`peekaboox-agent plugins --path examples/plugins`, `peekaboox-agent windows`,
and `peekaboox-agent desktop-state` expose the installed Python runtime and
daemon-facing diagnostics directly.

By default, daemon-routed real input injection is denied. Use
`peekabooxd run --profile operator`, `--allow-input`, or
`PEEKABOOX_ALLOW_INPUT=1` only for trusted local automation sessions. Audit logs
are written as JSONL; see `docs/security.md`.
Use `peekabooxd run --sandbox basic` for in-process Linux hardening, or install
`integrations/systemd/peekabooxd-hardened.service` for a stricter observe-only
systemd sandbox.

`peekabooxd` also starts a best-effort `CTRL + ALT + ESC` emergency hotkey
listener. When readable Linux input devices are available, the hotkey shuts the
daemon down and releases common modifier keys. Use `--no-emergency-hotkey` or
`PEEKABOOX_EMERGENCY_HOTKEY=0` in environments where `/dev/input/event*` access
is not available or not desired.
