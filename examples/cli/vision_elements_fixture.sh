#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURES="$ROOT/tests/fixtures/vision"
OUT_DIR="$ROOT/target/examples/vision-elements-fixture"

run_peekaboox() {
  if [[ -n "${PEEKABOOX_BIN:-}" ]]; then
    "$PEEKABOOX_BIN" "$@"
  elif command -v peekaboox >/dev/null 2>&1; then
    peekaboox "$@"
  else
    cargo run --quiet -p peekaboox-cli -- "$@"
  fi
}

require_file_contains() {
  local description="$1"
  local pattern="$2"
  local path="$3"
  if ! grep -Eq "$pattern" "$path"; then
    printf 'unexpected output for %s\npattern: %s\nfile: %s\n' \
      "$description" "$pattern" "$path" >&2
    cat "$path" >&2
    exit 1
  fi
}

controls="$FIXTURES/ui_controls.pbm"
if [[ ! -f "$controls" ]]; then
  echo "missing fixture: $controls" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

echo "== default fixture detection =="
default_json="$OUT_DIR/default.json"
run_peekaboox vision-elements "$controls" \
  --threshold 1 \
  --min-width 2 \
  --min-height 2 \
  --min-component-pixels 1 \
  --max-elements 20 \
  --merge-distance 1 \
  --json > "$default_json"
cat "$default_json"
require_file_contains "default detection role" '"role": "visual-region"' "$default_json"
require_file_contains "default first control" '"x": 4' "$default_json"
require_file_contains "default second control" '"x": 21' "$default_json"

echo "== ignored first control =="
ignored_json="$OUT_DIR/ignored.json"
run_peekaboox vision-elements "$controls" \
  --threshold 1 \
  --min-width 2 \
  --min-height 2 \
  --min-component-pixels 1 \
  --ignore-region 4,4,12,8 \
  --json > "$ignored_json"
cat "$ignored_json"
require_file_contains "ignored second control remains" '"x": 21' "$ignored_json"
if grep -Eq '"x": 4' "$ignored_json"; then
  echo "ignored output still contains the first control bounds" >&2
  exit 1
fi

echo "== confidence, size, area, and sort filters =="
filtered_json="$OUT_DIR/filtered.json"
run_peekaboox vision-elements "$controls" \
  --threshold 1 \
  --min-width 2 \
  --max-width 10 \
  --min-height 2 \
  --max-height 8 \
  --min-component-pixels 1 \
  --min-confidence 0.8 \
  --min-area 64 \
  --max-area 80 \
  --sort confidence \
  --json > "$filtered_json"
cat "$filtered_json"
require_file_contains "filtered second control" '"x": 21' "$filtered_json"

echo "== mask and overlay outputs =="
mask="$OUT_DIR/mask.png"
overlay="$OUT_DIR/overlay.png"
outputs_json="$OUT_DIR/outputs.json"
run_peekaboox vision-elements "$controls" \
  --threshold 1 \
  --min-width 2 \
  --min-height 2 \
  --min-component-pixels 1 \
  --padding 1 \
  --sort area \
  --mask-output "$mask" \
  --overlay-output "$overlay" \
  --json > "$outputs_json"
cat "$outputs_json"
test -s "$mask"
test -s "$overlay"
require_file_contains "output detection role" '"role": "visual-region"' "$outputs_json"

echo "PeekabooX vision-elements fixture example passed."
