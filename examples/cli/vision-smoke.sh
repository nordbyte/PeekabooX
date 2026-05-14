#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURES="$ROOT/tests/fixtures/vision"

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
  if ! grep -Eq "$pattern" <<<"$output"; then
    printf 'unexpected output for %s\npattern: %s\noutput:\n%s\n' \
      "$description" "$pattern" "$output" >&2
    exit 1
  fi
}

baseline="$FIXTURES/baseline.ppm"
changed="$FIXTURES/changed.ppm"
controls="$FIXTURES/ui_controls.pbm"
loading="$FIXTURES/ui_controls_loading.pbm"

for image in "$baseline" "$changed" "$controls" "$loading"; do
  if [[ ! -f "$image" ]]; then
    echo "missing fixture: $image" >&2
    exit 1
  fi
done

echo "== compare identical fixtures =="
compare_identical="$(run_peekaboox compare "$baseline" "$baseline")"
echo "$compare_identical"
require_output "identical compare" "matches=true" "$compare_identical"

echo "== compare changed fixtures with tolerated delta =="
compare_changed="$(run_peekaboox compare "$baseline" "$changed" --max-changed-ratio 1.0)"
echo "$compare_changed"
require_output "changed compare" "changed=[1-9][0-9]*/" "$compare_changed"

echo "== detect ui state across fixture sequence =="
state_output="$(
  run_peekaboox state \
    "$controls" \
    "$loading" \
    "$controls" \
    --stable-max-changed-ratio 0.0 \
    --loading-min-changed-ratio 0.000001
)"
echo "$state_output"
require_output "ui state" "state=(stable|changing|loading)" "$state_output"

echo "== detect visual UI elements from fixture =="
elements_output="$(
  run_peekaboox vision-elements \
    "$controls" \
    --threshold 1 \
    --min-width 2 \
    --min-height 2 \
    --min-component-pixels 1 \
    --max-elements 20 \
    --merge-distance 1
)"
echo "$elements_output"
require_output "vision elements" "visual-region" "$elements_output"

echo "PeekabooX CLI vision smoke example passed."
