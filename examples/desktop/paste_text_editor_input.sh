#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/paste-text-editor}}"
RUN_ID="${PEEKABOOX_PASTE_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
DRAFT_FILE="${PEEKABOOX_PASTE_DRAFT:-$OUT_DIR/peekaboox-paste-draft-$RUN_ID.txt}"
MESSAGE_FILE="$OUT_DIR/paste-message-$RUN_ID.txt"
TEXT_JSON="$OUT_DIR/paste-text-dry-run-$RUN_ID.json"
FILE_JSON="$OUT_DIR/paste-file-dry-run-$RUN_ID.json"
STDIN_JSON="$OUT_DIR/paste-stdin-dry-run-$RUN_ID.json"
AFTER_CAPTURE="$OUT_DIR/after-paste-$RUN_ID.png"
TEXT_EDITOR_LOG="$OUT_DIR/text-editor-$RUN_ID.log"
STRICT="${PEEKABOOX_STRICT:-0}"
LIVE="${PEEKABOOX_PASTE_LIVE:-0}"
CLIPBOARD_BACKEND="${PEEKABOOX_PASTE_CLIPBOARD_BACKEND:-auto}"
HOTKEY_BACKEND="${PEEKABOOX_PASTE_HOTKEY_BACKEND:-auto}"
DELAY_MS="${PEEKABOOX_PASTE_DELAY_MS:-80}"
RESTORE_DELAY_MS="${PEEKABOOX_PASTE_RESTORE_DELAY_MS:-120}"
RESTORE_POLICY="${PEEKABOOX_PASTE_RESTORE_POLICY:-strict}"
TEXT="${PEEKABOOX_PASTE_TEXT:-PeekabooX paste command example}"
SENTINEL="${PEEKABOOX_PASTE_SENTINEL:-PeekabooX clipboard sentinel $RUN_ID}"
FOCUS_WAIT_MS="${PEEKABOOX_PASTE_FOCUS_WAIT_MS:-500}"
LAUNCH_DELAY="${PEEKABOOX_PASTE_LAUNCH_DELAY:-2}"
SAVE_DELAY="${PEEKABOOX_PASTE_SAVE_DELAY:-1}"
WINDOW_TITLE_HINT="$(basename "$DRAFT_FILE")"
failures=0
text_editor_pid=""
clipboard_helper=""

run_peekaboox() {
  if [[ -n "${PEEKABOOX_BIN:-}" ]]; then
    "$PEEKABOOX_BIN" "$@"
  elif command -v peekaboox >/dev/null 2>&1; then
    peekaboox "$@"
  else
    cargo run --quiet -p peekaboox-cli -- "$@"
  fi
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

run_step_to_file() {
  local description="$1"
  local file="$2"
  shift 2
  printf '\n== %s ==\n' "$description"
  if "$@" > "$file"; then
    cat "$file"
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
  if [[ -n "${PEEKABOOX_PASTE_EDITOR_APP:-}" ]]; then
    printf '%s\n' "$PEEKABOOX_PASTE_EDITOR_APP"
    return 0
  fi
  if command -v gnome-text-editor >/dev/null 2>&1; then
    printf '%s\n' "gnome-text-editor"
    return 0
  fi
  return 1
}

choose_clipboard_helper() {
  case "$CLIPBOARD_BACKEND" in
    wl-copy)
      command -v wl-copy >/dev/null 2>&1 && command -v wl-paste >/dev/null 2>&1 && printf '%s\n' wl-copy
      ;;
    xclip)
      command -v xclip >/dev/null 2>&1 && printf '%s\n' xclip
      ;;
    xsel)
      command -v xsel >/dev/null 2>&1 && printf '%s\n' xsel
      ;;
    auto)
      if command -v wl-copy >/dev/null 2>&1 && command -v wl-paste >/dev/null 2>&1; then
        printf '%s\n' wl-copy
      elif command -v xclip >/dev/null 2>&1; then
        printf '%s\n' xclip
      elif command -v xsel >/dev/null 2>&1; then
        printf '%s\n' xsel
      fi
      ;;
  esac
}

set_clipboard_text() {
  case "$clipboard_helper" in
    wl-copy) printf '%s' "$1" | wl-copy ;;
    xclip) printf '%s' "$1" | xclip -selection clipboard ;;
    xsel) printf '%s' "$1" | xsel --clipboard --input ;;
    *) return 1 ;;
  esac
}

read_clipboard_text() {
  case "$clipboard_helper" in
    wl-copy) wl-paste --no-newline ;;
    xclip) xclip -selection clipboard -out ;;
    xsel) xsel --clipboard --output ;;
    *) return 1 ;;
  esac
}

run_stdin_paste_dry_run() {
  printf '%s\n' "$TEXT" | run_peekaboox paste --stdin --dry-run --json
}

maybe_close_text_editor() {
  if [[ "${PEEKABOOX_PASTE_CLOSE:-0}" == "1" ]]; then
    run_step "close text editor" run_peekaboox hotkey Alt+F4
  fi
}

mkdir -p "$OUT_DIR" "$(dirname "$DRAFT_FILE")"
for path in "$DRAFT_FILE" "$MESSAGE_FILE" "$TEXT_JSON" "$FILE_JSON" "$STDIN_JSON" "$AFTER_CAPTURE" "$TEXT_EDITOR_LOG"; do
  if [[ -e "$path" ]]; then
    echo "error: refusing to overwrite existing file: $path" >&2
    exit 1
  fi
done

printf 'PeekabooX paste command draft.\n' > "$DRAFT_FILE"
printf '%s\n' "$TEXT" > "$MESSAGE_FILE"

echo "PeekabooX paste command example output: $OUT_DIR"
echo "Draft file: $DRAFT_FILE"
echo "Message file: $MESSAGE_FILE"
echo "Live paste: $LIVE"
echo "Clipboard backend: $CLIPBOARD_BACKEND"
echo "Hotkey backend: $HOTKEY_BACKEND"

run_step_to_file "dry-run direct text paste" "$TEXT_JSON" \
  run_peekaboox paste \
    --dry-run \
    --json \
    --preserve-clipboard \
    --clipboard-backend "$CLIPBOARD_BACKEND" \
    --hotkey-backend "$HOTKEY_BACKEND" \
    --delay-ms "$DELAY_MS" \
    --restore-delay-ms "$RESTORE_DELAY_MS" \
    --restore-policy "$RESTORE_POLICY" \
    --text "$TEXT"

run_step_to_file "dry-run file paste" "$FILE_JSON" \
  run_peekaboox paste --dry-run --json --file "$MESSAGE_FILE"

run_step_to_file "dry-run stdin paste" "$STDIN_JSON" run_stdin_paste_dry_run

if [[ "$LIVE" != "1" ]]; then
  echo
  echo "Dry-run checks completed. Set PEEKABOOX_PASTE_LIVE=1 to paste into GNOME Text Editor."
  exit 0
fi

if ! clipboard_helper="$(choose_clipboard_helper)" || [[ -z "$clipboard_helper" ]]; then
  echo "warning: no readable/writable clipboard helper found for live restore verification" >&2
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
  exit 0
fi

if ! text_editor_app="$(find_text_editor_app)"; then
  echo "warning: GNOME Text Editor was not found; install gnome-text-editor" >&2
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
  exit 0
fi

require_step "set clipboard sentinel" set_clipboard_text "$SENTINEL"

"$text_editor_app" --standalone --ignore-session --new-window "$DRAFT_FILE" > "$TEXT_EDITOR_LOG" 2>&1 &
text_editor_pid="$!"
sleep "$LAUNCH_DELAY"

require_step "focus text editor" \
  run_peekaboox desktop focus --app text-editor --window-title "$WINDOW_TITLE_HINT" --no-launch --no-overview --wait-ms "$FOCUS_WAIT_MS"
require_step "focus document area" \
  run_peekaboox desktop click --app text-editor --target document --window-title "$WINDOW_TITLE_HINT" --verify
require_step "select draft content" run_peekaboox hotkey ctrl+a
require_step "paste live file text" \
  run_peekaboox paste \
    --preserve-clipboard \
    --clipboard-backend "$CLIPBOARD_BACKEND" \
    --hotkey-backend "$HOTKEY_BACKEND" \
    --delay-ms "$DELAY_MS" \
    --restore-delay-ms "$RESTORE_DELAY_MS" \
    --restore-policy "$RESTORE_POLICY" \
    --file "$MESSAGE_FILE"
run_step "save draft file" run_peekaboox hotkey ctrl+s
sleep "$SAVE_DELAY"
run_step "capture text editor after paste" run_peekaboox capture --output "$AFTER_CAPTURE"

if ! grep -Fq "$TEXT" "$DRAFT_FILE"; then
  failures=$((failures + 1))
  echo "warning: draft file does not contain expected text: $DRAFT_FILE" >&2
  if [[ "$STRICT" == "1" ]]; then
    maybe_close_text_editor
    exit 1
  fi
fi

if [[ "$RESTORE_POLICY" != "off" ]]; then
  restored="$(read_clipboard_text || true)"
  if [[ "$restored" != "$SENTINEL" ]]; then
    failures=$((failures + 1))
    echo "warning: clipboard was not restored to the sentinel text" >&2
    if [[ "$STRICT" == "1" ]]; then
      maybe_close_text_editor
      exit 1
    fi
  fi
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
  echo "PeekabooX paste command example passed."
fi
echo "Draft file: $DRAFT_FILE"
echo "Message file: $MESSAGE_FILE"
