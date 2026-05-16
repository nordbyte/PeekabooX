#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURES="$ROOT/tests/fixtures/vision"
OUT="${PEEKABOOX_EXAMPLE_OUTPUT:-$ROOT/target/examples/ui-state-sequence}"

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
small_white="$OUT/small-white.ppm"

mkdir -p "$OUT"

for image in "$baseline" "$changed" "$controls" "$loading"; do
  if [[ ! -f "$image" ]]; then
    echo "missing fixture: $image" >&2
    exit 1
  fi
done

cat >"$small_white" <<'PPM'
P3
2 2
255
255 255 255   255 255 255
255 255 255   255 255 255
PPM

echo "== stable identical sequence =="
stable="$(
  run_peekaboox state "$baseline" "$baseline" --json
)"
echo "$stable"
require_output "stable identical sequence" '"state": "stable"' "$stable"
require_output "stable identical sequence" '"stable_transitions": 1' "$stable"

echo "== loading fixture sequence =="
loading_state="$(
  run_peekaboox state "$controls" "$loading" "$controls" \
    --stable-max-changed-ratio 0.0 \
    --loading-min-changed-ratio 0.000001 \
    --json
)"
echo "$loading_state"
require_output "loading fixture sequence" '"state": "loading"' "$loading_state"
require_output "loading fixture sequence" '"loading_transitions": 2' "$loading_state"

echo "== ignore known volatile region =="
ignored="$(
  run_peekaboox state "$controls" "$loading" \
    --ignore-region 4,15,20,2 \
    --json
)"
echo "$ignored"
require_output "ignored UI-state region" '"state": "stable"' "$ignored"
require_output "ignored UI-state region" '"changed_pixels": 0' "$ignored"

echo "== absolute stable and loading pixel gates =="
pixel_gates="$(
  run_peekaboox state "$baseline" "$changed" \
    --stable-max-changed-ratio 1.0 \
    --stable-max-changed-pixels 1 \
    --stable-max-mae 255 \
    --stable-max-channel-delta 255 \
    --loading-min-changed-ratio 1.0 \
    --loading-min-changed-pixels 2 \
    --json
)"
echo "$pixel_gates"
require_output "absolute pixel gates" '"state": "loading"' "$pixel_gates"
require_output "absolute pixel gates" '"loading_transitions": 1' "$pixel_gates"

echo "== common-region size policy =="
common_region="$(
  run_peekaboox state "$baseline" "$small_white" \
    --size-policy common-region \
    --alpha ignore \
    --json
)"
echo "$common_region"
require_output "common-region UI state" '"state": "stable"' "$common_region"

echo "PeekabooX UI-state sequence example passed. Artifacts: $OUT"
