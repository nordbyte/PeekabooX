# PeekabooX Examples

This directory contains runnable examples for the current PeekabooX API surface.

## CI-safe examples

These examples do not require a live Linux desktop session and are suitable for
local smoke tests and CI:

```bash
bash examples/cli/vision-smoke.sh
bash examples/cli/ocr-smoke.sh
PYTHONPATH=python/src python3 examples/python/runtime_smoke.py
PYTHONPATH=python/src bash examples/mcp/jsonrpc_list_tools.sh
cargo run -q -p peekaboox-cli -- doctor --json
```

`examples/cli/vision-smoke.sh` uses the deterministic image fixtures under
`tests/fixtures/vision`. Set `PEEKABOOX_BIN=/path/to/peekaboox` to test an
installed or packaged binary instead of the local Cargo fallback.

`examples/cli/ocr-smoke.sh` uses `tests/fixtures/ocr/ocr_sample.png` to test
Tesseract-backed OCR over an image file, region OCR, JSON block metadata, and
word-level output. It skips cleanly when `tesseract` is not installed.

## Live desktop examples

These examples expect a Linux desktop session with matching capture,
accessibility, and input backends:

```bash
bash examples/desktop/live_smoke.sh
bash examples/desktop/ocr_visible_window.sh
bash examples/desktop/paint_draw_and_save.sh
bash examples/desktop/text_editor_save_dialog.sh
bash examples/desktop/telegram_saved_messages.sh
```

Set `PEEKABOOX_STRICT=1` when you want the live smoke script to fail on the
first backend error. Without strict mode, unavailable desktop capabilities are
reported as warnings so the script remains useful across Wayland, X11, and
headless environments.

`examples/desktop/paint_draw_and_save.sh` opens a blank PNG in `drawing`,
`pinta`, or `kolourpaint`, locates the canvas through `peekaboox desktop`,
draws with ratio-based `desktop drag` actions, saves with `hotkey ctrl+s`, falls
back to the detected Save toolbar button when needed, and verifies that the
output file changed. Override `PEEKABOOX_PAINT_APP` to force a specific paint
application.

`examples/desktop/ocr_visible_window.sh` opens
`examples/desktop/assets/ocr_desktop_sample.png` in the desktop's image viewer,
captures the visible desktop, runs live Tesseract-backed `peekaboox ocr` over
the screen, checks for `PX-OCR-204`, `READY`, and `VERIFY SCREEN TEXT`, and
writes text and JSON OCR outputs under `target/examples/desktop-ocr`. Override
`PEEKABOOX_DESKTOP_OCR_VIEWER` to force a specific viewer, or set
`PEEKABOOX_DESKTOP_OCR_WINDOW_TITLE`, `PEEKABOOX_DESKTOP_OCR_WINDOW_ID`, or
`PEEKABOOX_DESKTOP_OCR_APP` to scope OCR to a matching window.

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
