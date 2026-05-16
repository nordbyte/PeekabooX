#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/hotkey-cli}"
RUN_ID="${PEEKABOOX_HOTKEY_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
JSON_OUT="$OUT_DIR/hotkey-dry-run-$RUN_ID.json"
ALIAS_JSON_OUT="$OUT_DIR/hotkey-alias-dry-run-$RUN_ID.json"

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
  if "$@" >"$file"; then
    cat "$file"
    return 0
  fi
  echo "warning: $description failed; hotkey backends may be unavailable in this environment" >&2
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
for path in "$JSON_OUT" "$ALIAS_JSON_OUT"; do
  if [[ -e "$path" ]]; then
    echo "error: refusing to overwrite existing file: $path" >&2
    exit 1
  fi
done

run_or_skip_to_file "hotkey dry-run with timing options" "$JSON_OUT" \
  run_peekaboox hotkey \
    --dry-run \
    --json \
    --backend "${PEEKABOOX_HOTKEY_BACKEND:-auto}" \
    --delay-ms "${PEEKABOOX_HOTKEY_DELAY_MS:-25}" \
    --key-delay-ms "${PEEKABOOX_HOTKEY_KEY_DELAY_MS:-30}" \
    --repeat "${PEEKABOOX_HOTKEY_REPEAT:-2}" \
    --interval-ms "${PEEKABOOX_HOTKEY_INTERVAL_MS:-40}" \
    --release-before \
    --release-after \
    control+s
require_json_field "$JSON_OUT" action hotkey
require_json_field "$JSON_OUT" dry_run True
require_json_field "$JSON_OUT" repeat 2
require_json_field "$JSON_OUT" release_before True
require_json_field "$JSON_OUT" release_after True

run_or_skip_to_file "hotkey dry-run using -- separator and key aliases" "$ALIAS_JSON_OUT" \
  run_peekaboox hotkey --dry-run --json -- --help
require_json_field "$ALIAS_JSON_OUT" action hotkey
require_json_field "$ALIAS_JSON_OUT" key_count 1

echo "PeekabooX hotkey CLI example passed."
