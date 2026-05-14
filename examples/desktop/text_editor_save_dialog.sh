#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/text-editor-save}}"
RUN_ID="${PEEKABOOX_TEXT_EDITOR_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
DRAFT_FILE="${PEEKABOOX_TEXT_EDITOR_DRAFT:-$OUT_DIR/peekaboox-text-editor-draft-$RUN_ID.txt}"
OUT_FILE="${PEEKABOOX_TEXT_EDITOR_OUTPUT:-$OUT_DIR/peekaboox-text-editor-saved-$RUN_ID}"
AUTO_EXTENSION_OUT_FILE="$OUT_FILE.txt"
AFTER_TYPE_CAPTURE="$OUT_DIR/after-type-$RUN_ID.png"
AFTER_SAVE_CAPTURE="$OUT_DIR/after-save-$RUN_ID.png"
TEXT_EDITOR_LOG="$OUT_DIR/text-editor-$RUN_ID.log"
STRICT="${PEEKABOOX_STRICT:-0}"
TEXT="${PEEKABOOX_TEXT_EDITOR_TEXT:-PeekabooX TextEditor Save Dialog Example}"
LAUNCH_DELAY="${PEEKABOOX_TEXT_EDITOR_LAUNCH_DELAY:-2}"
FOCUS_WAIT_MS="${PEEKABOOX_TEXT_EDITOR_FOCUS_WAIT_MS:-500}"
SAVE_DIALOG_DELAY="${PEEKABOOX_TEXT_EDITOR_SAVE_DIALOG_DELAY:-1}"
SAVE_DELAY="${PEEKABOOX_TEXT_EDITOR_SAVE_DELAY:-1}"
SAVE_HOTKEY="${PEEKABOOX_TEXT_EDITOR_SAVE_HOTKEY:-ctrl+shift+s}"
failures=0
text_editor_pid=""
saved_file=""
WINDOW_TITLE_HINT="$(basename "$DRAFT_FILE")"

run_peekaboox() {
  if [[ -n "${PEEKABOOX_BIN:-}" ]]; then
    "$PEEKABOOX_BIN" "$@"
  elif command -v peekaboox >/dev/null 2>&1; then
    peekaboox "$@"
  else
    cargo run --quiet -p peekaboox-cli -- "$@"
  fi
}

copy_to_clipboard() {
  local text="$1"
  if command -v wl-copy >/dev/null 2>&1; then
    printf '%s' "$text" | wl-copy
    return 0
  fi

  if command -v xclip >/dev/null 2>&1; then
    printf '%s' "$text" | xclip -selection clipboard
    return 0
  fi

  if command -v xsel >/dev/null 2>&1; then
    printf '%s' "$text" | xsel --clipboard --input
    return 0
  fi

  return 1
}

paste_text() {
  local text="$1"
  if copy_to_clipboard "$text"; then
    run_peekaboox hotkey ctrl+v
    return 0
  fi

  run_peekaboox type "$text"
}

run_step() {
  local description="$1"
  shift
  printf '\n== %s ==\n' "$description"
  if "$@"; then
    return 0
  fi

  failures=$((failures + 1))
  if [[ "$STRICT" == "1" ]]; then
    echo "failed: $description" >&2
    exit 1
  fi
  echo "warning: $description failed in this desktop environment" >&2
  return 0
}

require_step() {
  local description="$1"
  shift
  printf '\n== %s ==\n' "$description"
  if "$@"; then
    return 0
  fi

  failures=$((failures + 1))
  echo "warning: $description failed" >&2
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
  exit 0
}

find_text_editor_app() {
  if [[ -n "${PEEKABOOX_TEXT_EDITOR_APP:-}" ]]; then
    printf '%s\n' "$PEEKABOOX_TEXT_EDITOR_APP"
    return 0
  fi

  if command -v gnome-text-editor >/dev/null 2>&1; then
    printf '%s\n' "gnome-text-editor"
    return 0
  fi

  return 1
}

launch_text_editor_app() {
  local app="$1"
  "$app" --standalone --ignore-session --new-window "$DRAFT_FILE" >"$TEXT_EDITOR_LOG" 2>&1 &
  text_editor_pid="$!"
}

maybe_close_text_editor() {
  if [[ "${PEEKABOOX_TEXT_EDITOR_CLOSE:-0}" == "1" ]]; then
    local close_title="$WINDOW_TITLE_HINT"
    if [[ -n "$saved_file" ]]; then
      close_title="$(basename "$saved_file")"
    elif [[ -f "$OUT_FILE" ]]; then
      close_title="$(basename "$OUT_FILE")"
    elif [[ -f "$AUTO_EXTENSION_OUT_FILE" ]]; then
      close_title="$(basename "$AUTO_EXTENSION_OUT_FILE")"
    fi

    printf '\n== focus text editor before close ==\n'
    if run_peekaboox desktop focus --app text-editor --window-title "$close_title" --no-launch --no-overview --wait-ms "$FOCUS_WAIT_MS"; then
      run_step "close text editor" run_peekaboox hotkey Alt+F4
      return 0
    fi

    failures=$((failures + 1))
    echo "warning: refusing to send Alt+F4 because the expected text editor window is not focused" >&2
    if [[ "$STRICT" == "1" ]]; then
      exit 1
    fi
  fi
}

mkdir -p "$OUT_DIR" "$(dirname "$DRAFT_FILE")" "$(dirname "$OUT_FILE")" "$(dirname "$TEXT_EDITOR_LOG")"

if [[ "$DRAFT_FILE" == "$OUT_FILE" ]] || [[ "$DRAFT_FILE" == "$AUTO_EXTENSION_OUT_FILE" ]]; then
  echo "error: draft and output file must be different paths" >&2
  exit 1
fi

for path in "$DRAFT_FILE" "$OUT_FILE" "$AUTO_EXTENSION_OUT_FILE" "$AFTER_TYPE_CAPTURE" "$AFTER_SAVE_CAPTURE" "$TEXT_EDITOR_LOG"; do
  if [[ -e "$path" ]]; then
    echo "error: refusing to overwrite existing file: $path" >&2
    exit 1
  fi
done

printf 'PeekabooX draft file for safe window targeting.\n' >"$DRAFT_FILE"

echo "PeekabooX Text Editor example output: $OUT_DIR"
echo "Draft file: $DRAFT_FILE"
echo "Requested save path: $OUT_FILE"
echo "Accepted auto-extension path: $AUTO_EXTENSION_OUT_FILE"
echo "Window title hint: $WINDOW_TITLE_HINT"
echo "Text: $TEXT"

if ! text_editor_app="$(find_text_editor_app)"; then
  echo "warning: GNOME Text Editor was not found; install gnome-text-editor" >&2
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
  exit 0
fi

echo "Launching text editor app: $text_editor_app"
launch_text_editor_app "$text_editor_app"
sleep "$LAUNCH_DELAY"

run_step "window enumeration after launch" run_peekaboox windows
require_step "focus text editor" \
  run_peekaboox desktop focus --app text-editor --window-title "$WINDOW_TITLE_HINT" --no-launch --no-overview --wait-ms "$FOCUS_WAIT_MS"
require_step "locate text document" \
  run_peekaboox desktop locate --app text-editor --target document --window-title "$WINDOW_TITLE_HINT"
require_step "type example text" \
  run_peekaboox desktop type-into --app text-editor --target document --window-title "$WINDOW_TITLE_HINT" --clear "$TEXT"
run_step "capture text editor after typing" run_peekaboox capture --output "$AFTER_TYPE_CAPTURE"

require_step "open save dialog" run_peekaboox hotkey "$SAVE_HOTKEY"
sleep "$SAVE_DIALOG_DELAY"
require_step "open save dialog location entry" run_peekaboox hotkey ctrl+l
require_step "paste absolute save path" paste_text "$OUT_FILE"
require_step "confirm save path" run_peekaboox hotkey Enter
sleep "$SAVE_DELAY"
run_step "capture desktop after save" run_peekaboox capture --output "$AFTER_SAVE_CAPTURE"

for candidate in "$OUT_FILE" "$AUTO_EXTENSION_OUT_FILE"; do
  if [[ -f "$candidate" ]]; then
    saved_file="$candidate"
    break
  fi
done

if [[ -z "$saved_file" ]]; then
  failures=$((failures + 1))
  echo "warning: expected output file was not created: $OUT_FILE or $AUTO_EXTENSION_OUT_FILE" >&2
  if [[ "$STRICT" == "1" ]]; then
    maybe_close_text_editor
    exit 1
  fi
elif ! grep -Fq "$TEXT" "$saved_file"; then
  failures=$((failures + 1))
  echo "warning: output file does not contain expected text: $saved_file" >&2
  if [[ "$STRICT" == "1" ]]; then
    maybe_close_text_editor
    exit 1
  fi
else
  echo "Saved text file: $saved_file"
fi

maybe_close_text_editor

if [[ -n "$text_editor_pid" ]] && ! kill -0 "$text_editor_pid" >/dev/null 2>&1; then
  text_editor_pid=""
fi

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX Text Editor save-dialog example passed."
fi
