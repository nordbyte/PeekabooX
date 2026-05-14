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
bash examples/desktop/telegram_saved_messages.sh
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

`examples/desktop/telegram_saved_messages.sh` opens Telegram Desktop, focuses
the global search bar, searches for `Saved Messages`, opens the result, and
sends `PeekabooX Example` to that chat. Telegram must already be logged in.
Override `PEEKABOOX_TELEGRAM_MESSAGE`, `PEEKABOOX_TELEGRAM_SEARCH_QUERY`,
`PEEKABOOX_TELEGRAM_APP`, or the optional coordinate variables
`PEEKABOOX_TELEGRAM_FOCUS_X/Y`, `PEEKABOOX_TELEGRAM_SEARCH_X/Y`,
`PEEKABOOX_TELEGRAM_CLEAR_X/Y`, `PEEKABOOX_TELEGRAM_RESULT_X/Y`,
`PEEKABOOX_TELEGRAM_INPUT_X/Y`, and `PEEKABOOX_TELEGRAM_SEND_X/Y` when the
default keyboard path does not match your Telegram build, locale, or desktop
focus behavior. By default the script refuses to type unless a Telegram window
is focused; set
`PEEKABOOX_TELEGRAM_REQUIRE_FOCUS=0` to bypass that focus guard, or
`PEEKABOOX_TELEGRAM_WINDOW_GUARD=0` when your desktop does not expose Telegram
through window enumeration and you provide reliable coordinates.

`examples/workflows/desktop_observe.yaml` and
`examples/workflows/input_actions.yaml` are editable workflow files for
daemon-backed runtime or MCP execution.
