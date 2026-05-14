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
```

Set `PEEKABOOX_STRICT=1` when you want the live smoke script to fail on the
first backend error. Without strict mode, unavailable desktop capabilities are
reported as warnings so the script remains useful across Wayland, X11, and
headless environments.

`examples/workflows/desktop_observe.yaml` is an editable workflow file for
daemon-backed runtime or MCP execution.
