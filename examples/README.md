# PeekabooX Examples

This directory contains runnable examples for the current PeekabooX API surface.

## CI-safe examples

These examples do not require a live Linux desktop session and are suitable for
local smoke tests and CI:

```bash
bash examples/cli/vision-smoke.sh
PYTHONPATH=python/src python3 examples/python/runtime_smoke.py
PYTHONPATH=python/src bash examples/mcp/jsonrpc_list_tools.sh
```

`examples/cli/vision-smoke.sh` uses the deterministic image fixtures under
`tests/fixtures/vision`. Set `PEEKABOOX_BIN=/path/to/peekaboox` to test an
installed or packaged binary instead of the local Cargo fallback.

## Live desktop examples

These examples expect a Linux desktop session with matching capture,
accessibility, and input backends:

```bash
bash examples/desktop/live_smoke.sh
bash examples/desktop/paint_draw_and_save.sh
```

Set `PEEKABOOX_STRICT=1` when you want the live smoke script to fail on the
first backend error. Without strict mode, unavailable desktop capabilities are
reported as warnings so the script remains useful across Wayland, X11, and
headless environments.

`examples/desktop/paint_draw_and_save.sh` opens a blank PNG in `drawing`,
`pinta`, or `kolourpaint`, draws with `move` and `drag`, saves with
`hotkey ctrl+s`, falls back to the visible Save toolbar button when needed, and
verifies that the output file changed. Override `PEEKABOOX_PAINT_APP`,
`PEEKABOOX_PAINT_CANVAS_X`, `PEEKABOOX_PAINT_CANVAS_Y`,
`PEEKABOOX_PAINT_SAVE_X`, or `PEEKABOOX_PAINT_SAVE_Y` if your desktop layout
needs different coordinates.

`examples/workflows/desktop_observe.yaml` and
`examples/workflows/input_actions.yaml` are editable workflow files for
daemon-backed runtime or MCP execution.
