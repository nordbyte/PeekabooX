#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/hotkey-text-editor}}"
RUN_ID="${PEEKABOOX_HOTKEY_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
DRAFT_FILE="${PEEKABOOX_HOTKEY_DRAFT:-$OUT_DIR/peekaboox-hotkey-draft-$RUN_ID.txt}"
DRY_RUN_JSON="$OUT_DIR/hotkey-dry-run-$RUN_ID.json"
AFTER_CAPTURE="$OUT_DIR/after-hotkey-save-$RUN_ID.png"
TEXT_EDITOR_LOG="$OUT_DIR/text-editor-$RUN_ID.log"
STRICT="${PEEKABOOX_STRICT:-0}"
LIVE="${PEEKABOOX_HOTKEY_LIVE:-0}"
HOTKEY_BACKEND="${PEEKABOOX_HOTKEY_BACKEND:-auto}"
TYPE_BACKEND="${PEEKABOOX_TYPE_BACKEND:-auto}"
TEXT="${PEEKABOOX_HOTKEY_TEXT:-PeekabooX hotkey save example}"
FOCUS_WAIT_MS="${PEEKABOOX_HOTKEY_FOCUS_WAIT_MS:-500}"
LAUNCH_DELAY="${PEEKABOOX_HOTKEY_LAUNCH_DELAY:-2}"
SAVE_DELAY="${PEEKABOOX_HOTKEY_SAVE_DELAY:-1}"
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

find_text_editor_app() {
  if [[ -n "${PEEKABOOX_HOTKEY_EDITOR_APP:-}" ]]; then
    printf '%s\n' "$PEEKABOOX_HOTKEY_EDITOR_APP"
    return 0
  fi
  if command -v gnome-text-editor >/dev/null 2>&1; then
    printf '%s\n' "gnome-text-editor"
    return 0
  fi
  return 1
}

mkdir -p "$OUT_DIR" "$(dirname "$DRAFT_FILE")"
for path in "$DRAFT_FILE" "$DRY_RUN_JSON" "$AFTER_CAPTURE" "$TEXT_EDITOR_LOG"; do
  if [[ -e "$path" ]]; then
    echo "error: refusing to overwrite existing file: $path" >&2
    exit 1
  fi
done

printf 'PeekabooX hotkey command draft.\n' >"$DRAFT_FILE"

echo "PeekabooX hotkey desktop example output: $OUT_DIR"
echo "Draft file: $DRAFT_FILE"
echo "Live save: $LIVE"
echo "Hotkey backend: $HOTKEY_BACKEND"

run_step_to_file "dry-run ctrl+s with timing and modifier release" "$DRY_RUN_JSON" \
  run_peekaboox hotkey \
    --dry-run \
    --json \
    --backend "$HOTKEY_BACKEND" \
    --delay-ms 25 \
    --key-delay-ms 30 \
    --repeat 1 \
    --release-before \
    --release-after \
    control+s

if [[ "$LIVE" != "1" ]]; then
  echo
  echo "Dry-run check completed. Set PEEKABOOX_HOTKEY_LIVE=1 to save a GNOME Text Editor draft with ctrl+s."
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
require_step "select draft content" run_peekaboox hotkey --backend "$HOTKEY_BACKEND" ctrl+a
require_step "type replacement text" \
  run_peekaboox type --backend "$TYPE_BACKEND" --typing-speed 20 --text "$TEXT"
require_step "save draft with ctrl+s" \
  run_peekaboox hotkey --backend "$HOTKEY_BACKEND" --release-before --release-after ctrl+s
sleep "$SAVE_DELAY"
run_step "capture text editor after hotkey save" run_peekaboox capture --output "$AFTER_CAPTURE"

if ! grep -Fq "$TEXT" "$DRAFT_FILE"; then
  failures=$((failures + 1))
  echo "warning: draft file does not contain expected text: $DRAFT_FILE" >&2
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
fi

if [[ "$failures" -gt 0 ]]; then
  echo "PeekabooX hotkey desktop example completed with $failures warning(s)."
else
  echo "PeekabooX hotkey desktop example passed."
fi

if [[ -n "$text_editor_pid" ]]; then
  echo "Text editor PID left running: $text_editor_pid"
fi
