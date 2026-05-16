#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/paste-sources}"
RUN_ID="${PEEKABOOX_PASTE_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
MESSAGE_FILE="$OUT_DIR/paste-message-$RUN_ID.txt"
TEXT_JSON="$OUT_DIR/paste-text-$RUN_ID.json"
FILE_JSON="$OUT_DIR/paste-file-$RUN_ID.json"
STDIN_JSON="$OUT_DIR/paste-stdin-$RUN_ID.json"

run_peekaboox() {
  if [[ -n "${PEEKABOOX_BIN:-}" ]]; then
    "$PEEKABOOX_BIN" "$@"
  elif command -v peekaboox >/dev/null 2>&1; then
    peekaboox "$@"
  else
    cargo run --quiet -p peekaboox-cli -- "$@"
  fi
}

run_or_skip_to_file() {
  local description="$1"
  local file="$2"
  shift 2
  echo "== $description =="
  if "$@" > "$file"; then
    cat "$file"
    return 0
  fi
  echo "warning: $description failed; paste backends may be unavailable in this environment" >&2
  if [[ "${PEEKABOOX_STRICT:-0}" == "1" ]]; then
    exit 1
  fi
  exit 0
}

require_json_field() {
  local file="$1"
  local field="$2"
  local expected="$3"
  python3 - "$file" "$field" "$expected" <<'PY'
import json
import sys

path, field, expected = sys.argv[1:4]
data = json.loads(open(path, encoding="utf-8").read())
value = data
for part in field.split("."):
    value = value[part]
if str(value) != expected:
    raise SystemExit(f"{field} expected {expected!r}, got {value!r}")
PY
}

mkdir -p "$OUT_DIR"
for path in "$MESSAGE_FILE" "$TEXT_JSON" "$FILE_JSON" "$STDIN_JSON"; do
  if [[ -e "$path" ]]; then
    echo "error: refusing to overwrite existing file: $path" >&2
    exit 1
  fi
done

printf 'PeekabooX paste file source\n' > "$MESSAGE_FILE"

run_or_skip_to_file "direct text source" "$TEXT_JSON" \
  run_peekaboox paste \
  --dry-run \
  --json \
  --preserve-clipboard \
  --clipboard-backend "${PEEKABOOX_PASTE_CLIPBOARD_BACKEND:-auto}" \
  --hotkey-backend "${PEEKABOOX_PASTE_HOTKEY_BACKEND:-auto}" \
  --delay-ms "${PEEKABOOX_PASTE_DELAY_MS:-80}" \
  --restore-delay-ms "${PEEKABOOX_PASTE_RESTORE_DELAY_MS:-120}" \
  --restore-policy "${PEEKABOOX_PASTE_RESTORE_POLICY:-strict}" \
  --text "PeekabooX paste direct source"
require_json_field "$TEXT_JSON" action paste_text
require_json_field "$TEXT_JSON" dry_run True

run_or_skip_to_file "file source" "$FILE_JSON" \
  run_peekaboox paste --dry-run --json --file "$MESSAGE_FILE"
require_json_field "$FILE_JSON" action paste_text

run_stdin_dry_run() {
  printf 'PeekabooX paste stdin source\n' | run_peekaboox paste --dry-run --json --stdin
}

run_or_skip_to_file "stdin source" "$STDIN_JSON" run_stdin_dry_run
require_json_field "$STDIN_JSON" action paste_text

echo "PeekabooX paste sources CLI example passed."
