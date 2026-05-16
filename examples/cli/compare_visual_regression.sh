#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURES="$ROOT/tests/fixtures/vision"
OUT="${PEEKABOOX_EXAMPLE_OUTPUT:-$ROOT/target/examples/compare-visual-regression}"

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
small_white="$OUT/small-white.ppm"
diff_image="$OUT/changed-diff.png"
report="$OUT/changed-report.json"

mkdir -p "$OUT"

for image in "$baseline" "$changed"; do
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

echo "== compare identical fixture =="
identical="$(run_peekaboox compare "$baseline" "$baseline" --json)"
echo "$identical"
require_output "identical compare" '"matches": true' "$identical"

echo "== strict comparison fails for changed fixture =="
if run_peekaboox compare "$baseline" "$changed" >"$OUT/strict-failure.txt" 2>&1; then
  echo "strict comparison unexpectedly matched changed fixture" >&2
  exit 1
fi
cat "$OUT/strict-failure.txt"

echo "== tolerated visual regression emits report and diff mask =="
tolerated="$(
  run_peekaboox compare "$baseline" "$changed" \
    --max-changed-ratio 0.2 \
    --max-changed-pixels 2 \
    --max-mae 30 \
    --max-channel-delta 255 \
    --diff-output "$diff_image" \
    --report "$report" \
    --json
)"
echo "$tolerated"
require_output "tolerated compare" '"matches": true' "$tolerated"
require_output "tolerated compare" '"changed_pixels": 2' "$tolerated"
test -s "$diff_image"
test -s "$report"

echo "== ignore known volatile region =="
ignored="$(
  run_peekaboox compare "$baseline" "$changed" \
    --ignore-region 1,1,2,1 \
    --json
)"
echo "$ignored"
require_output "ignored region compare" '"matches": true' "$ignored"
require_output "ignored region compare" '"changed_pixels": 0' "$ignored"

echo "== compare stable header region only =="
region_only="$(
  run_peekaboox compare "$baseline" "$changed" \
    --region 0,0,4,1 \
    --json
)"
echo "$region_only"
require_output "region-only compare" '"matches": true' "$region_only"

echo "== compare different dimensions through common region =="
common_region="$(
  run_peekaboox compare "$baseline" "$small_white" \
    --size-policy common-region \
    --alpha ignore \
    --json
)"
echo "$common_region"
require_output "common-region compare" '"matches": true' "$common_region"

echo "== no-fail keeps report-style runs non-blocking =="
run_peekaboox compare "$baseline" "$changed" \
  --max-changed-ratio 0.0 \
  --max-changed-pixels 0 \
  --no-fail \
  --json >"$OUT/no-fail.json"
require_output "no-fail compare" '"matches": false' "$(cat "$OUT/no-fail.json")"

echo "PeekabooX visual regression compare example passed. Artifacts: $OUT"
