#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_ID="${PEEKABOOX_CAPTURE_DELTA_RUN_ID:-$(date +%Y%m%d-%H%M%S)}"
OUT_ROOT="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/capture-delta}"
OUT_DIR="$OUT_ROOT/$RUN_ID"
STRICT="${PEEKABOOX_STRICT:-0}"
REGION="${PEEKABOOX_CAPTURE_DELTA_REGION:-0,0,320,180}"
STREAM_PREFIX="${PEEKABOOX_CAPTURE_DELTA_STREAM_PREFIX:-live-smoke-$RUN_ID}"
PRIMARY_STREAM="$STREAM_PREFIX-primary"
SECONDARY_STREAM="$STREAM_PREFIX-secondary"
REGION_STREAM="$STREAM_PREFIX-region"
SOCKET="$OUT_DIR/peekabooxd.sock"
AUDIT_LOG="$OUT_DIR/peekabooxd-audit.jsonl"
DAEMON_PID=""
failures=0

fresh_binary() {
  local binary="$1"
  shift
  [[ -x "$binary" ]] || return 1
  local source
  for source in "$@"; do
    [[ "$binary" -nt "$source" ]] || return 1
  done
  return 0
}

run_peekaboox() {
  if [[ -n "${PEEKABOOX_BIN:-}" ]]; then
    "$PEEKABOOX_BIN" "$@"
  elif fresh_binary \
    "$ROOT/target/debug/peekaboox" \
    "$ROOT/cli/src/main.rs" \
    "$ROOT/rust/ipc/src/lib.rs"; then
    "$ROOT/target/debug/peekaboox" "$@"
  elif command -v cargo >/dev/null 2>&1; then
    cargo run --quiet -p peekaboox-cli -- "$@"
  else
    peekaboox "$@"
  fi
}

run_peekabooxd() {
  if [[ -n "${PEEKABOOXD_BIN:-}" ]]; then
    "$PEEKABOOXD_BIN" "$@"
  elif fresh_binary \
    "$ROOT/target/debug/peekabooxd" \
    "$ROOT/rust/daemon/src/main.rs" \
    "$ROOT/rust/capture/src/lib.rs" \
    "$ROOT/rust/ipc/src/lib.rs"; then
    "$ROOT/target/debug/peekabooxd" "$@"
  elif command -v cargo >/dev/null 2>&1; then
    cargo run --quiet -p peekabooxd -- "$@"
  else
    peekabooxd "$@"
  fi
}

cleanup() {
  if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

record_failure() {
  local description="$1"
  failures=$((failures + 1))
  if [[ "$STRICT" == "1" ]]; then
    echo "failed: $description" >&2
    cleanup
    exit 1
  fi
  echo "warning: $description failed in this desktop environment" >&2
}

run_step() {
  local description="$1"
  shift
  printf '\n== %s ==\n' "$description"
  if "$@"; then
    return 0
  fi
  record_failure "$description"
  return 0
}

start_daemon() {
  run_peekabooxd run \
    --profile observe \
    --socket "$SOCKET" \
    --audit-log "$AUDIT_LOG" \
    --no-grpc \
    --no-emergency-hotkey &
  DAEMON_PID=$!

  for _ in $(seq 1 80); do
    if [[ -S "$SOCKET" ]]; then
      return 0
    fi
    if ! kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
      return 1
    fi
    sleep 0.1
  done

  return 1
}

capture_delta_json() {
  local output="$1"
  shift
  run_peekaboox --daemon --socket "$SOCKET" capture-delta --json "$@" >"$output"
}

validate_delta() {
  local path="$1"
  local stream="$2"
  local sequence="$3"
  local full_frame="$4"
  local low_bandwidth="$5"
  local expected_region="$6"

  python3 - "$path" "$stream" "$sequence" "$full_frame" "$low_bandwidth" "$expected_region" <<'PY'
import base64
import json
import sys

path, stream, sequence, full_frame, low_bandwidth, expected_region = sys.argv[1:7]
with open(path, "r", encoding="utf-8") as handle:
    payload = json.load(handle)

expected_sequence = int(sequence)
expected_full_frame = full_frame == "true"
expected_low_bandwidth = low_bandwidth == "true"

if payload.get("stream_id") != stream:
    raise SystemExit(f"stream_id mismatch: {payload.get('stream_id')} != {stream}")
if payload.get("sequence") != expected_sequence:
    raise SystemExit(f"sequence mismatch: {payload.get('sequence')} != {expected_sequence}")
if payload.get("full_frame") is not expected_full_frame:
    raise SystemExit(f"full_frame mismatch: {payload.get('full_frame')} != {expected_full_frame}")
if payload.get("low_bandwidth") is not expected_low_bandwidth:
    raise SystemExit(
        f"low_bandwidth mismatch: {payload.get('low_bandwidth')} != {expected_low_bandwidth}"
    )
if payload.get("frame_width", 0) <= 0 or payload.get("frame_height", 0) <= 0:
    raise SystemExit("frame dimensions must be positive")
if not payload.get("backend_name"):
    raise SystemExit("backend_name is missing")

patch_base64 = payload.get("patch_base64", "")
try:
    base64.b64decode(patch_base64, validate=True)
except Exception as exc:
    raise SystemExit(f"patch_base64 is invalid: {exc}") from exc
if expected_full_frame and not patch_base64:
    raise SystemExit("full-frame delta must include patch data")

region = payload.get("capture_region")
if expected_region == "-":
    if region is not None:
        raise SystemExit(f"unexpected capture_region: {region}")
else:
    x, y, width, height = [int(part) for part in expected_region.replace("x", ",").split(",")]
    expected = {"x": x, "y": y, "width": width, "height": height}
    if region != expected:
        raise SystemExit(f"capture_region mismatch: {region} != {expected}")
    if payload.get("frame_width") != width or payload.get("frame_height") != height:
        raise SystemExit(
            f"region frame size mismatch: {payload.get('frame_width')}x{payload.get('frame_height')}"
        )

print(
    "stream={stream} sequence={sequence} full_frame={full_frame} low_bandwidth={low_bandwidth} "
    "frame={width}x{height} changed_pixels={changed_pixels} backend={backend}".format(
        stream=payload["stream_id"],
        sequence=payload["sequence"],
        full_frame=payload["full_frame"],
        low_bandwidth=payload["low_bandwidth"],
        width=payload["frame_width"],
        height=payload["frame_height"],
        changed_pixels=payload["changed_pixels"],
        backend=payload["backend_name"],
    )
)
PY
}

if [[ -e "$OUT_DIR" ]]; then
  echo "output directory already exists: $OUT_DIR" >&2
  exit 1
fi
mkdir -p "$OUT_DIR"

PRIMARY_RESET_JSON="$OUT_DIR/primary-reset.json"
PRIMARY_DELTA_JSON="$OUT_DIR/primary-delta.json"
PRIMARY_FULL_JSON="$OUT_DIR/primary-forced-full.json"
PRIMARY_AFTER_FULL_JSON="$OUT_DIR/primary-after-full.json"
SECONDARY_RESET_JSON="$OUT_DIR/secondary-reset.json"
PRIMARY_AFTER_SECONDARY_JSON="$OUT_DIR/primary-after-secondary.json"
REGION_RESET_JSON="$OUT_DIR/region-reset.json"
REGION_DELTA_JSON="$OUT_DIR/region-delta.json"

echo "PeekabooX capture-delta live stream output: $OUT_DIR"
echo "Primary stream: $PRIMARY_STREAM"
echo "Secondary stream: $SECONDARY_STREAM"
echo "Region stream: $REGION_STREAM"
echo "Region: $REGION"

run_step "start observe daemon" start_daemon

run_step "primary reset full-frame capture" \
  capture_delta_json "$PRIMARY_RESET_JSON" --stream "$PRIMARY_STREAM" --reset --low-bandwidth
run_step "validate primary reset full-frame capture" \
  validate_delta "$PRIMARY_RESET_JSON" "$PRIMARY_STREAM" 1 true true -

run_step "primary follow-up low-bandwidth delta" \
  capture_delta_json "$PRIMARY_DELTA_JSON" --stream "$PRIMARY_STREAM" --low-bandwidth
run_step "validate primary follow-up low-bandwidth delta" \
  validate_delta "$PRIMARY_DELTA_JSON" "$PRIMARY_STREAM" 2 false true -

run_step "primary forced full-frame capture" \
  capture_delta_json "$PRIMARY_FULL_JSON" --stream "$PRIMARY_STREAM" --full-frame
run_step "validate primary forced full-frame capture" \
  validate_delta "$PRIMARY_FULL_JSON" "$PRIMARY_STREAM" 3 true false -

run_step "primary delta after forced full-frame" \
  capture_delta_json "$PRIMARY_AFTER_FULL_JSON" --stream "$PRIMARY_STREAM" --low-bandwidth
run_step "validate primary delta after forced full-frame" \
  validate_delta "$PRIMARY_AFTER_FULL_JSON" "$PRIMARY_STREAM" 4 false true -

run_step "secondary independent stream reset" \
  capture_delta_json "$SECONDARY_RESET_JSON" --stream "$SECONDARY_STREAM" --reset --low-bandwidth
run_step "validate secondary independent stream reset" \
  validate_delta "$SECONDARY_RESET_JSON" "$SECONDARY_STREAM" 1 true true -

run_step "primary stream remains independent" \
  capture_delta_json "$PRIMARY_AFTER_SECONDARY_JSON" --stream "$PRIMARY_STREAM" --low-bandwidth
run_step "validate primary stream remains independent" \
  validate_delta "$PRIMARY_AFTER_SECONDARY_JSON" "$PRIMARY_STREAM" 5 false true -

run_step "region stream reset full-frame capture" \
  capture_delta_json "$REGION_RESET_JSON" --stream "$REGION_STREAM" --reset --region "$REGION"
run_step "validate region stream reset full-frame capture" \
  validate_delta "$REGION_RESET_JSON" "$REGION_STREAM" 1 true true "$REGION"

run_step "region stream follow-up delta" \
  capture_delta_json "$REGION_DELTA_JSON" --stream "$REGION_STREAM" --region "$REGION"
run_step "validate region stream follow-up delta" \
  validate_delta "$REGION_DELTA_JSON" "$REGION_STREAM" 2 false true "$REGION"

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX capture-delta live stream example passed."
fi
