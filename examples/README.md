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

`examples/desktop/telegram_saved_messages.sh` opens or focuses Telegram Desktop
through GNOME Overview when available, locates Telegram's search field, the
Saved Messages result, the message input, and the send button from the current
screenshot, and sends `PeekabooX Example` to that chat. Telegram must already
be logged in. The coordinate-free layout detection uses
`examples/desktop/telegram_layout.py` and requires Python Pillow in live desktop
environments. Override `PEEKABOOX_TELEGRAM_MESSAGE`,
`PEEKABOOX_TELEGRAM_APP_SEARCH_QUERY`, `PEEKABOOX_TELEGRAM_APP`,
`PEEKABOOX_TELEGRAM_USE_GNOME_OVERVIEW`, or
`PEEKABOOX_TELEGRAM_USE_SAVED_MESSAGES_URI` to change the default flow. The
`tg://savedmessages` URI is disabled by default because some Telegram Desktop
builds focus the app without switching chats; set
`PEEKABOOX_TELEGRAM_SKIP_SEARCH=1` only when you have separately guaranteed
that Saved Messages is already open. Manual coordinate variables
`PEEKABOOX_TELEGRAM_FOCUS_X/Y`,
`PEEKABOOX_TELEGRAM_OVERVIEW_RESULT_X/Y`,
`PEEKABOOX_TELEGRAM_SEARCH_X/Y`, `PEEKABOOX_TELEGRAM_CLEAR_X/Y`,
`PEEKABOOX_TELEGRAM_RESULT_X/Y`, `PEEKABOOX_TELEGRAM_INPUT_X/Y`, and
`PEEKABOOX_TELEGRAM_SEND_X/Y` remain available as last-resort overrides.
`PEEKABOOX_TELEGRAM_WINDOW_GUARD` defaults to `0` because some GNOME Wayland
and Snap combinations do not expose Telegram through window enumeration.

`examples/workflows/desktop_observe.yaml` and
`examples/workflows/input_actions.yaml` are editable workflow files for
daemon-backed runtime or MCP execution.
