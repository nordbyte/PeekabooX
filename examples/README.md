# PeekabooX Examples

This directory contains runnable examples for the current PeekabooX API surface.

## CI-safe examples

These examples do not require a live Linux desktop session and are suitable for
local smoke tests and CI:

```bash
bash examples/cli/vision-smoke.sh
bash examples/cli/vision_elements_fixture.sh
bash examples/cli/compare_visual_regression.sh
bash examples/cli/ui_state_sequence.sh
bash examples/cli/ocr-smoke.sh
bash examples/cli/agent-preflight-smoke.sh
bash examples/cli/paste_sources.sh
PYTHONPATH=python/src python3 examples/python/runtime_smoke.py
PYTHONPATH=python/src python3 examples/python/doctor_runtime.py
PYTHONPATH=python/src python3 examples/python/paste_runtime.py
PYTHONPATH=python/src bash examples/mcp/jsonrpc_list_tools.sh
PYTHONPATH=python/src bash examples/mcp/jsonrpc_doctor.sh
PYTHONPATH=python/src bash examples/mcp/jsonrpc_preflight.sh
PYTHONPATH=python/src bash examples/mcp/jsonrpc_preflight_error_client.sh
PYTHONPATH=python/src bash examples/mcp/jsonrpc_resources.sh
PYTHONPATH=python/src bash examples/mcp/jsonrpc_prompts.sh
PYTHONPATH=python/src bash examples/mcp/jsonrpc_recovery_matrix.sh
PYTHONPATH=python/src bash examples/mcp/jsonrpc_tool_parity.sh
PYTHONPATH=python/src bash examples/mcp/jsonrpc_image_content.sh
PYTHONPATH=python/src bash examples/mcp/jsonrpc_paste_text.sh
cargo run -q -p peekaboox-cli -- doctor --json
```

`examples/cli/vision-smoke.sh` uses the deterministic image fixtures under
`tests/fixtures/vision`. Set `PEEKABOOX_BIN=/path/to/peekaboox` to test an
installed or packaged binary instead of the local Cargo fallback.

`examples/cli/vision_elements_fixture.sh` focuses on the standalone
`vision-elements` command. It validates deterministic fixture detection,
ignore regions, confidence/size/area filters, sorting, padding, and mask plus
overlay debug outputs under `target/examples/vision-elements-fixture`.

`examples/cli/compare_visual_regression.sh` exercises the visual regression
surface on deterministic fixtures: strict failures, tolerated changed-pixel and
MAE gates, repeatable ignore regions, region-only comparisons, size policies,
JSON reports, diff-mask output, and `--no-fail` report mode. It writes artifacts
under `target/examples/compare-visual-regression`.

`examples/cli/ui_state_sequence.sh` validates deterministic UI-state detection:
stable, loading, ignored volatile regions, absolute stable/loading pixel gates,
and common-region size policy. It writes temporary fixture output under
`target/examples/ui-state-sequence`.

`examples/cli/ocr-smoke.sh` uses `tests/fixtures/ocr/ocr_sample.png` to test
Tesseract-backed OCR over an image file, region OCR, JSON block metadata, and
word-level output. It skips cleanly when `tesseract` is not installed.

`examples/cli/agent-preflight-smoke.sh` exercises
`peekaboox-agent --preflight-mode strict preflight desktop capture` and
validates the JSON result shape. Set `PEEKABOOX_STRICT=1` to make blocked
Doctor categories fail the example instead of being reported.

`examples/cli/paste_sources.sh` validates the `paste` CLI surface in dry-run
mode with direct text, file, and stdin sources plus clipboard backend,
hotkey backend, delay, restore delay, and restore-policy options.

`examples/cli/hotkey_dry_run.sh` validates the `hotkey` CLI surface in dry-run
mode with backend selection, timing, repeats, modifier release, JSON output,
alias normalization, and the `--` separator for dash-prefixed key names.

`examples/cli/plugins_system_info.sh` validates the Plugin SDK CLI surface. It
discovers `examples/plugins/system-info` from both the plugin directory and the
manifest path, checks the declared tool schema, executes
`system_info.uname` through `peekaboox plugin-call`, and writes JSON artifacts
under `target/examples/plugins-system-info`.

`examples/python/doctor_runtime.py` and `examples/mcp/jsonrpc_doctor.sh` run the
same structured environment diagnostics through `AgentRuntime.doctor(...)` and
MCP JSON-RPC `tools/call`. They validate per-check categories, severities, and
category rollups, then write JSON under
`target/examples/python-doctor` or `target/examples/mcp-doctor`. Set
`PEEKABOOX_BIN` to test an installed binary instead of the local Cargo fallback.
`examples/python/paste_runtime.py` checks the Python runtime and workflow
surface for `paste_text` without mutating the desktop by using a recording
client and `dry_run=True`.
`examples/python/hotkey_runtime.py` checks the Python runtime and workflow
surface for `hotkey`, including normalized aliases, dry-runs, backend/timing
options, repeats, and modifier release flags.
`examples/python/plugins_runtime.py` checks Plugin SDK discovery and execution
through `AgentRuntime.list_plugins(...)` and `AgentRuntime.call_plugin_tool(...)`
with explicit plugin paths and bounded output.
`examples/mcp/jsonrpc_preflight.sh` uses those Doctor categories through the MCP
`preflight` tool before a capture-style operation.
`examples/mcp/jsonrpc_preflight_error_client.sh` injects a deterministic Doctor
failure, calls `click` through MCP with strict preflight, and shows how an MCP
client can branch on `PreflightError.next_action` and `blocked_categories`
without parsing prose error text.
`examples/mcp/jsonrpc_resources.sh`, `jsonrpc_prompts.sh`,
`jsonrpc_recovery_matrix.sh`, `jsonrpc_tool_parity.sh`, and
`jsonrpc_image_content.sh` cover the wider MCP surface: resources/templates,
prompts, completion, logging, structured recovery for security/preflight
failures, CLI-compatible tool aliases, tool annotations/output schemas, and MCP
image content blocks. `jsonrpc_paste_text.sh` verifies the MCP `paste_text`
schema for clipboard/hotkey backend selection, timing, dry-run, and restore
policy fields, with an optional live dry-run call when
`PEEKABOOX_MCP_PASTE_LIVE=1`. `jsonrpc_hotkey.sh` verifies the MCP `hotkey`
schema for backend selection, timing, repeats, dry-run, and modifier release
fields, with an optional dry-run daemon call when `PEEKABOOX_MCP_HOTKEY_LIVE=1`.
`jsonrpc_plugins.sh` validates the MCP `list_plugins` and `call_plugin_tool`
schemas, discovers the system-info example plugin, and executes its read-only
tool through JSON-RPC.

## Live desktop examples

These examples expect a Linux desktop session with matching capture,
accessibility, and input backends:

```bash
bash examples/desktop/live_smoke.sh
bash examples/desktop/capture_backends_diagnostics.sh
./examples/desktop/capture_window_targets.sh
./examples/desktop/capture_daemon_mcp_targets.sh
python3 examples/python/capture_backends_runtime.py
bash examples/mcp/jsonrpc_capture_backends.sh
bash examples/desktop/capture_delta_stream.sh
python3 examples/python/capture_delta_runtime.py
bash examples/mcp/jsonrpc_capture_delta.sh
bash examples/desktop/windows_inventory.sh
bash examples/desktop/desktop_profiles_registry.sh
bash examples/desktop/desktop_profiles_daemon_parity.sh
bash examples/desktop/elements_accessibility_probe.sh
bash examples/desktop/elements_calculator.sh
bash examples/desktop/ocr_visible_window.sh
bash examples/desktop/move_pointer_path.sh
bash examples/desktop/click_calculator_keypad.sh
bash examples/desktop/type_text_editor_input.sh
bash examples/desktop/paste_text_editor_input.sh
bash examples/desktop/hotkey_text_editor_save.sh
bash examples/desktop/paint_draw_and_save.sh
bash examples/desktop/text_editor_save_dialog.sh
bash examples/desktop/telegram_saved_messages.sh
```

Set `PEEKABOOX_STRICT=1` when you want the live smoke script to fail on the
first backend error. Without strict mode, unavailable desktop capabilities are
reported as warnings so the script remains useful across Wayland, X11, and
headless environments.

`examples/desktop/capture_backends_diagnostics.sh` runs
`peekaboox capture-backends --diagnose --json`, probes file, frame, region, and
optional DMA-BUF capture paths, and checks the daemon-routed command surface
through a temporary observe-only `peekabooxd` socket. It writes per-run JSON
reports and probe output under `target/examples/capture-backends`. Override
`PEEKABOOX_CAPTURE_BACKENDS_REGION` to change the region probe.

`examples/desktop/capture_window_targets.sh` opens or reuses Calculator and
validates the main `capture` command surface: app/title-regex window targeting,
window-relative regions, JSON metadata, semantic-tree metadata, PNG stdout,
XWD output, and `--no-overwrite`. It writes artifacts under
`target/examples/capture-window-targets`. Override `PEEKABOOX_CAPTURE_APP`,
`PEEKABOOX_CAPTURE_APP_QUERY`, or `PEEKABOOX_CAPTURE_TITLE_REGEX` for custom
desktops.

`examples/desktop/capture_daemon_mcp_targets.sh` starts a temporary observe-only
daemon and validates the same capture targeting through daemon IPC, Python
`AgentRuntime.capture_screen(...)`, and MCP JSON-RPC `capture_screen`. It checks
app/title-regex targeting, window-relative regions, daemon JSON metadata,
semantic-tree metadata, XWD output, and daemon `--no-overwrite`. It writes
artifacts under `target/examples/capture-daemon-mcp-targets`. Override
`PEEKABOOX_CAPTURE_PARITY_APP_QUERY`, `PEEKABOOX_CAPTURE_PARITY_TITLE_REGEX`,
or `PEEKABOOX_PYTHON_BIN` for custom desktops or Python runtimes.

`examples/python/capture_backends_runtime.py` and
`examples/mcp/jsonrpc_capture_backends.sh` run matching capture-backends
discovery and file/frame/region probe checks through `AgentRuntime.connect(...)`
and MCP JSON-RPC `tools/call` respectively. Both start a temporary observe-only
daemon with gRPC and write JSON responses under
`target/examples/python-capture-backends` or
`target/examples/mcp-capture-backends`. Set `PEEKABOOX_PYTHON_BIN` when your
system Python does not already have the packaged `grpcio` and `protobuf`
dependencies.

`examples/desktop/capture_delta_stream.sh` starts a temporary observe-only
daemon and validates `peekaboox --daemon capture-delta --json` across reset
full-frame captures, follow-up low-bandwidth deltas, forced full-frame requests,
independent stream state, and region-scoped streams. It writes per-step JSON
responses under `target/examples/capture-delta`. Override
`PEEKABOOX_CAPTURE_DELTA_REGION` to change the region stream.

`examples/python/capture_delta_runtime.py` and
`examples/mcp/jsonrpc_capture_delta.sh` run the same capture-delta stream checks
through `AgentRuntime.connect(...)` and MCP JSON-RPC `tools/call` respectively.
Both start a temporary observe-only daemon with gRPC and write JSON responses
under `target/examples/python-capture-delta` or
`target/examples/mcp-capture-delta`. Set `PEEKABOOX_PYTHON_BIN` when your system
Python does not already have the packaged `grpcio` and `protobuf` dependencies.

`examples/desktop/paint_draw_and_save.sh` opens a blank PNG in `drawing`,
`pinta`, or `kolourpaint`, locates the canvas through `peekaboox desktop`,
draws with ratio-based `desktop drag` actions, saves with `hotkey ctrl+s`, falls
back to the detected Save toolbar button when needed, and verifies that the
output file changed. Override `PEEKABOOX_PAINT_APP` to force a specific paint
application.

`examples/desktop/drag_absolute_canvas.sh` opens a blank PNG in a supported
paint application, locates the canvas, converts the canvas rectangle into
absolute drag coordinates, and exercises the raw `peekaboox drag` command with
JSON dry-runs, scoped ratio endpoints, backend selection, step control, bounds
clamping, and optional cursor restore. It runs as dry-run by default; set
`PEEKABOOX_DRAG_LIVE=1` to draw on the live canvas.

`examples/desktop/move_pointer_path.sh` exercises the `move` command surface:
cursor position JSON, compact `--to`, region-ratio targeting, relative deltas,
smooth movement options, bounds policies, and backend selection. It runs as
dry-run by default; set `PEEKABOOX_MOVE_POINTER_LIVE=1` to move the real pointer
and restore the original cursor position at the end.

`examples/desktop/click_calculator_keypad.sh` opens GNOME Calculator, resolves the
digit `7` button through `peekaboox elements`, and exercises the raw
`peekaboox click` command with semantic selectors, absolute `--to` coordinates,
window-scoped `--ratio`, JSON dry-runs, backend selection, bounds clamping, and
cursor restore. It runs as dry-run by default; set `PEEKABOOX_CLICK_LIVE=1` to
perform a real Calculator button click.

`examples/desktop/type_text_editor_input.sh` exercises the raw `peekaboox type`
command with `--text`, `--file`, `--stdin`, JSON dry-runs, backend selection,
typing speed, initial delay, and per-key delay. It runs as dry-run by default;
set `PEEKABOOX_TYPE_LIVE=1` to open GNOME Text Editor on a unique draft file,
focus the document, type the sample text, save, and verify the file content.

`examples/desktop/hotkey_text_editor_save.sh` exercises the raw
`peekaboox hotkey` command with JSON dry-runs, backend selection, initial and
per-key timing, alias normalization, and modifier release flags. It runs as
dry-run by default; set `PEEKABOOX_HOTKEY_LIVE=1` to open GNOME Text Editor on
a unique draft file, replace its text, save with `ctrl+s`, and verify the file
content. The editor window is left open.

`examples/desktop/windows_inventory.sh` runs the enhanced `peekaboox windows`
command with backend diagnostics, focused-window filtering, Calculator
app/title-regex matching, id lookup, and `--window-id` handoff into `capture`
and `elements`. It writes JSON, the resolved window id, and an optional window
capture under `target/examples/windows-inventory`. Override
`PEEKABOOX_WINDOWS_BACKEND`, `PEEKABOOX_WINDOWS_APP_QUERY`,
`PEEKABOOX_WINDOWS_TITLE_REGEX`, or `PEEKABOOX_WINDOWS_CALCULATOR_APP` for
custom desktops.

`examples/desktop/desktop_profiles_registry.sh` validates the desktop profile
registry without opening applications. It checks JSON schema/count fields,
filters by app, target, command, and target capability, verifies that launch
command arguments such as `flatpak run org.telegram.desktop` are preserved, and
exercises `--availability` metadata. It writes JSON reports under
`target/examples/desktop-profiles`.

`examples/desktop/desktop_profiles_daemon_parity.sh` starts a temporary
observe-only daemon with a short Unix socket path and gRPC enabled, then checks
the same `desktop_profiles` query through daemon CLI IPC, `PeekabooXClient`,
and MCP JSON-RPC. It validates schema/count fields, `message-input`
`type-into` support, command argument preservation, and availability metadata,
then writes all three JSON responses under
`target/examples/desktop-profiles-daemon-parity`. Set `PEEKABOOX_PYTHON_BIN`
when your system Python does not already have `grpcio`, `protobuf`, and the
local PeekabooX package importable.

`examples/desktop/ocr_visible_window.sh` opens
`examples/desktop/assets/ocr_desktop_sample.png` in the desktop's image viewer,
captures the visible desktop, runs live Tesseract-backed `peekaboox ocr` over
the screen, checks for `PX-OCR-204`, `READY`, and `VERIFY SCREEN TEXT`, and
writes text and JSON OCR outputs under `target/examples/desktop-ocr`. Override
`PEEKABOOX_DESKTOP_OCR_VIEWER` to force a specific viewer, or set
`PEEKABOOX_DESKTOP_OCR_WINDOW_TITLE`, `PEEKABOOX_DESKTOP_OCR_WINDOW_ID`, or
`PEEKABOOX_DESKTOP_OCR_APP` to scope OCR to a matching window.

`examples/desktop/elements_accessibility_probe.sh` opens a small GTK probe
window with a label, entry, checkbox, and button, then verifies `peekaboox
elements` against that live accessibility tree. It covers `--window-title`
scoping, exact and regex selectors, negative state filters, returned center
points through `--contains`, and configurable `--vision-fallback` detector
options. It writes JSON outputs under `target/examples/elements-probe` and
skips cleanly when Python GTK bindings are unavailable.

`examples/desktop/elements_calculator.sh` opens GNOME Calculator as a real
desktop application, scopes `peekaboox elements` to `--app gnome-calculator`
and `--window-title Calculator`, finds the digit buttons with exact and regex
selectors, derives a safe `peekaboox click --selector ... --dry-run` from the
exact button selector, reuses the returned `center` point with `--contains`,
and exercises window-scoped `--vision-fallback`. It writes JSON and dry-run
outputs under `target/examples/elements-calculator` and skips cleanly when
GNOME Calculator is not installed.

`examples/desktop/text_editor_save_dialog.sh` launches GNOME Text Editor on a
unique draft file, locates the document target while passing
`--window-title <draft>` to the desktop commands, types example text, opens the
native Save dialog, saves to a unique absolute path via the dialog's location
entry, accepts GNOME Text Editor's automatic `.txt` extension, and verifies the
resulting file content. The save path is inserted with `peekaboox paste`, which
uses `wl-copy`, `xclip`, or `xsel` plus the safest available paste hotkey backend
so keyboard layout mapping cannot corrupt path separators. It refuses to
overwrite existing files. Override
`PEEKABOOX_TEXT_EDITOR_TEXT`, `PEEKABOOX_TEXT_EDITOR_OUTPUT`, or
`PEEKABOOX_TEXT_EDITOR_FOCUS_WAIT_MS` for custom runs.

`examples/desktop/paste_text_editor_input.sh` is the dedicated live `paste`
example. It performs dry-run checks for direct text, file, and stdin sources,
then, when `PEEKABOOX_PASTE_LIVE=1`, opens GNOME Text Editor on a unique draft
file, sets a clipboard sentinel, pastes file-backed text with
`--preserve-clipboard`, saves the draft, verifies the file content, and checks
that the textual clipboard was restored. Override
`PEEKABOOX_PASTE_CLIPBOARD_BACKEND`, `PEEKABOOX_PASTE_HOTKEY_BACKEND`,
`PEEKABOOX_PASTE_RESTORE_POLICY`, or `PEEKABOOX_PASTE_TEXT` for custom runs.

`examples/desktop/telegram_saved_messages.sh` opens or focuses Telegram Desktop
through the reusable `peekaboox desktop focus` helper, locates Telegram's
search field, the Saved Messages result, the message input, and the send button
with `peekaboox desktop ... --target ...`, and sends `PeekabooX Example` to
that chat. Telegram must already be logged in. The example no longer depends
on hard-coded coordinates or Python Pillow; its Telegram layout resolver lives
in the `peekaboox-desktop` Rust crate. The same helper commands can be scoped
with `--window-id <id>` when multiple matching app windows are visible, and
mutating helper actions support `--verify` for post-action checks. Override
`PEEKABOOX_TELEGRAM_MESSAGE`,
`PEEKABOOX_TELEGRAM_SEARCH_QUERY`, `PEEKABOOX_TELEGRAM_FOCUS_WAIT_MS`, or
`PEEKABOOX_TELEGRAM_OVERVIEW_WAIT_MS` for slower desktops. Set
`PEEKABOOX_TELEGRAM_ASSERT_HEADER=1` to additionally verify the `Saved
Messages` header through accessibility or OCR when those desktop capabilities
are available.

`examples/workflows/desktop_observe.yaml` and
`examples/workflows/input_actions.yaml` are editable workflow files for
daemon-backed runtime or MCP execution.
