#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/type-text-editor}}"
RUN_ID="${PEEKABOOX_TYPE_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
DRAFT_FILE="${PEEKABOOX_TYPE_DRAFT:-$OUT_DIR/peekaboox-type-draft-$RUN_ID.txt}"
MESSAGE_FILE="$OUT_DIR/type-message-$RUN_ID.txt"
DRY_RUN_JSON="$OUT_DIR/type-dry-run-$RUN_ID.json"
FILE_DRY_RUN_JSON="$OUT_DIR/type-file-dry-run-$RUN_ID.json"
STDIN_DRY_RUN_JSON="$OUT_DIR/type-stdin-dry-run-$RUN_ID.json"
AFTER_CAPTURE="$OUT_DIR/after-type-$RUN_ID.png"
TEXT_EDITOR_LOG="$OUT_DIR/text-editor-$RUN_ID.log"
STRICT="${PEEKABOOX_STRICT:-0}"
LIVE="${PEEKABOOX_TYPE_LIVE:-0}"
BACKEND="${PEEKABOOX_TYPE_BACKEND:-auto}"
TYPING_SPEED="${PEEKABOOX_TYPE_SPEED:-20}"
DELAY_MS="${PEEKABOOX_TYPE_DELAY_MS:-50}"
TEXT="${PEEKABOOX_TYPE_TEXT:-PeekabooX type command example}"
FOCUS_WAIT_MS="${PEEKABOOX_TYPE_FOCUS_WAIT_MS:-500}"
LAUNCH_DELAY="${PEEKABOOX_TYPE_LAUNCH_DELAY:-2}"
SAVE_DELAY="${PEEKABOOX_TYPE_SAVE_DELAY:-1}"
WINDOW_TITLE_HINT="$(basename "$DRAFT_FILE")"
failures=0
text_editor_pid=""

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
  if "$@" >"$file"; then
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

run_stdin_type_dry_run() {
  printf '%s\n' "$TEXT" | run_peekaboox type --stdin --dry-run --json
}

find_text_editor_app() {
  if [[ -n "${PEEKABOOX_TYPE_EDITOR_APP:-}" ]]; then
    printf '%s\n' "$PEEKABOOX_TYPE_EDITOR_APP"
    return 0
  fi

  if command -v gnome-text-editor >/dev/null 2>&1; then
    printf '%s\n' "gnome-text-editor"
    return 0
  fi

  return 1
}

maybe_close_text_editor() {
  if [[ "${PEEKABOOX_TYPE_CLOSE:-0}" == "1" ]]; then
    run_step "close text editor" run_peekaboox hotkey Alt+F4
  fi
}

mkdir -p "$OUT_DIR" "$(dirname "$DRAFT_FILE")"

for path in "$DRAFT_FILE" "$MESSAGE_FILE" "$DRY_RUN_JSON" "$FILE_DRY_RUN_JSON" "$STDIN_DRY_RUN_JSON" "$AFTER_CAPTURE" "$TEXT_EDITOR_LOG"; do
  if [[ -e "$path" ]]; then
    echo "error: refusing to overwrite existing file: $path" >&2
    exit 1
  fi
done

printf 'PeekabooX type command draft.\n' >"$DRAFT_FILE"
printf '%s\n' "$TEXT" >"$MESSAGE_FILE"

echo "PeekabooX type command example output: $OUT_DIR"
echo "Draft file: $DRAFT_FILE"
echo "Message file: $MESSAGE_FILE"
echo "Live typing: $LIVE"
echo "Backend: $BACKEND"

run_step_to_file "dry-run direct text typing" "$DRY_RUN_JSON" \
  run_peekaboox type \
    --backend "$BACKEND" \
    --typing-speed "$TYPING_SPEED" \
    --delay-ms "$DELAY_MS" \
    --dry-run \
    --json \
    --text "$TEXT"

run_step_to_file "dry-run file text typing" "$FILE_DRY_RUN_JSON" \
  run_peekaboox type \
    --backend "$BACKEND" \
    --key-delay-ms 10 \
    --dry-run \
    --json \
    --file "$MESSAGE_FILE"

run_step_to_file "dry-run stdin text typing" "$STDIN_DRY_RUN_JSON" \
  run_stdin_type_dry_run

if [[ "$LIVE" != "1" ]]; then
  echo
  echo "Dry-run checks completed. Set PEEKABOOX_TYPE_LIVE=1 to type into GNOME Text Editor."
  exit 0
fi

if ! text_editor_app="$(find_text_editor_app)"; then
  echo "warning: GNOME Text Editor was not found; install gnome-text-editor" >&2
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
  exit 0
fi

"$text_editor_app" --standalone --ignore-session --new-window "$DRAFT_FILE" >"$TEXT_EDITOR_LOG" 2>&1 &
text_editor_pid="$!"
sleep "$LAUNCH_DELAY"

require_step "focus text editor" \
  run_peekaboox desktop focus --app text-editor --window-title "$WINDOW_TITLE_HINT" --no-launch --no-overview --wait-ms "$FOCUS_WAIT_MS"
require_step "focus document area" \
  run_peekaboox desktop click --app text-editor --target document --window-title "$WINDOW_TITLE_HINT" --verify
require_step "select draft content" run_peekaboox hotkey ctrl+a
require_step "type live text" \
  run_peekaboox type \
    --backend "$BACKEND" \
    --typing-speed "$TYPING_SPEED" \
    --delay-ms "$DELAY_MS" \
    --text "$TEXT"
run_step "save draft file" run_peekaboox hotkey ctrl+s
sleep "$SAVE_DELAY"
run_step "capture text editor after typing" run_peekaboox capture --output "$AFTER_CAPTURE"

if ! grep -Fq "$TEXT" "$DRAFT_FILE"; then
  failures=$((failures + 1))
  echo "warning: draft file does not contain expected text: $DRAFT_FILE" >&2
  if [[ "$STRICT" == "1" ]]; then
    maybe_close_text_editor
    exit 1
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
  echo "PeekabooX type command example passed."
fi
