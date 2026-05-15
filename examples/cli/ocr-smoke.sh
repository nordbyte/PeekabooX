#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE="$ROOT/tests/fixtures/ocr/ocr_sample.png"

run_peekaboox() {
  if [[ -n "${PEEKABOOX_BIN:-}" ]]; then
    "$PEEKABOOX_BIN" "$@"
  elif command -v peekaboox >/dev/null 2>&1; then
    peekaboox "$@"
  else
    cargo run --quiet -p peekaboox-cli -- "$@"
  fi
}

require_output() {
  local description="$1"
  local pattern="$2"
  local output="$3"
  if ! grep -Eiq "$pattern" <<<"$output"; then
    printf 'unexpected output for %s\npattern: %s\noutput:\n%s\n' \
      "$description" "$pattern" "$output" >&2
    exit 1
  fi
}

if [[ ! -f "$FIXTURE" ]]; then
  echo "missing OCR fixture: $FIXTURE" >&2
  exit 1
fi

if ! command -v tesseract >/dev/null 2>&1; then
  echo "skipping OCR smoke example: tesseract is not available"
  exit 0
fi

echo "== OCR full image text =="
text_output="$(
  run_peekaboox ocr \
    --image "$FIXTURE" \
    --language eng \
    --psm 6 \
    --min-confidence 0.20 \
    --scale 2 \
    --grayscale
)"
echo "$text_output"
require_output "full OCR text" "PeekabooX[[:space:]]+OCR[[:space:]]+Example" "$text_output"
require_output "full OCR invoice" "Invoice[[:space:]]+PX-104" "$text_output"
require_output "full OCR total" "Total[[:space:]]+42[.]17[[:space:]]+EUR" "$text_output"

echo "== OCR region JSON blocks and words =="
json_output="$(
  run_peekaboox ocr \
    --image "$FIXTURE" \
    --region 50,145,520,130 \
    --language eng \
    --psm 6 \
    --min-confidence 0.20 \
    --threshold 180 \
    --json
)"
echo "$json_output"
require_output "region OCR JSON" '"text"[[:space:]]*:[[:space:]]*"[^"]*Invoice[[:space:]]+PX-104' "$json_output"
require_output "region OCR words" '"words"[[:space:]]*:' "$json_output"
require_output "region OCR confidence" '"confidence"[[:space:]]*:' "$json_output"

echo "== OCR word table =="
words_output="$(
  run_peekaboox ocr \
    --image "$FIXTURE" \
    --region 50,145,520,130 \
    --language eng \
    --psm 6 \
    --words
)"
echo "$words_output"
require_output "word-level OCR" "Invoice" "$words_output"
require_output "word-level OCR" "PX-104" "$words_output"

echo "PeekabooX CLI OCR smoke example passed."
