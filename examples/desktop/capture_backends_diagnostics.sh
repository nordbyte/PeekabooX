#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_ID="${PEEKABOOX_CAPTURE_BACKENDS_RUN_ID:-$(date +%Y%m%d-%H%M%S)}"
OUT_ROOT="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/capture-backends}"
OUT_DIR="$OUT_ROOT/$RUN_ID"
STRICT="${PEEKABOOX_STRICT:-0}"
REGION="${PEEKABOOX_CAPTURE_BACKENDS_REGION:-0,0,320,180}"
failures=0
DAEMON_PID=""

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
    "$ROOT/rust/capture/src/lib.rs" \
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

cleanup() {
  if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

write_discovery_json() {
  local output="$1"
  local capture_output="$2"
  run_peekaboox capture-backends --diagnose --json --output "$capture_output" >"$output"
}

write_probe_json() {
  local probe="$1"
  local output="$2"
  local capture_output="$3"
  run_peekaboox capture-backends \
    --diagnose \
    --json \
    --probe "$probe" \
    --region "$REGION" \
    --output "$capture_output" >"$output"
}

summarize_discovery() {
  python3 - "$1" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

usable = [
    backend["name"]
    for backend in payload.get("image_backends", [])
    if backend.get("available") and backend.get("supports_output")
]
if not usable:
    raise SystemExit("no usable image capture backend reported")

zero_copy = payload.get("zero_copy_backends", [])
print(
    "session={session} desktop={desktop} image_backends={backends} zero_copy={zero_copy}".format(
        session=payload.get("session_type", "unknown"),
        desktop=payload.get("desktop") or "-",
        backends=",".join(usable),
        zero_copy=",".join(
            f"{backend.get('name')}:{backend.get('availability')}"
            for backend in zero_copy
        ) or "-",
    )
)
PY
}

assert_probe_ok() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

path, expected = sys.argv[1:3]
with open(path, "r", encoding="utf-8") as handle:
    payload = json.load(handle)

probes = [probe for probe in payload.get("probes", []) if probe.get("probe") == expected]
if not probes:
    raise SystemExit(f"missing probe result: {expected}")
probe = probes[0]
if not probe.get("ok"):
    raise SystemExit(f"{expected} probe failed: {probe.get('detail')}")
print(
    "{name} probe ok via {backend} {size}".format(
        name=expected,
        backend=probe.get("backend_name") or "-",
        size=(
            f"{probe.get('width')}x{probe.get('height')}"
            if probe.get("width") and probe.get("height")
            else "-"
        ),
    )
)
PY
}

should_probe_dmabuf() {
  python3 - "$1" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

if not payload.get("pipewire_backend_feature_enabled"):
    raise SystemExit(1)
for backend in payload.get("zero_copy_backends", []):
    if backend.get("availability") == "available" and backend.get("selected"):
        raise SystemExit(0)
raise SystemExit(1)
PY
}

start_daemon() {
  local socket="$1"
  local audit_log="$2"
  run_peekabooxd run \
    --profile observe \
    --socket "$socket" \
    --audit-log "$audit_log" \
    --no-grpc \
    --no-emergency-hotkey &
  DAEMON_PID=$!

  for _ in $(seq 1 80); do
    if [[ -S "$socket" ]]; then
      return 0
    fi
    if ! kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
      return 1
    fi
    sleep 0.1
  done

  return 1
}

write_daemon_discovery_json() {
  local socket="$1"
  local output="$2"
  local capture_output="$3"
  run_peekaboox --daemon --socket "$socket" \
    capture-backends --diagnose --json --output "$capture_output" >"$output"
}

if [[ -e "$OUT_DIR" ]]; then
  echo "output directory already exists: $OUT_DIR" >&2
  exit 1
fi
mkdir -p "$OUT_DIR"

DISCOVERY_JSON="$OUT_DIR/backends.json"
DAEMON_JSON="$OUT_DIR/backends-daemon.json"
FILE_PROBE_JSON="$OUT_DIR/probe-file.json"
FRAME_PROBE_JSON="$OUT_DIR/probe-frame.json"
REGION_PROBE_JSON="$OUT_DIR/probe-region.json"
DMABUF_PROBE_JSON="$OUT_DIR/probe-dmabuf.json"
SCREEN_PNG="$OUT_DIR/screen.png"
DAEMON_SCREEN_PNG="$OUT_DIR/daemon-screen.png"
SOCKET="$OUT_DIR/peekabooxd.sock"
AUDIT_LOG="$OUT_DIR/peekabooxd-audit.jsonl"

echo "PeekabooX capture backend diagnostics output: $OUT_DIR"
echo "Probe region: $REGION"

run_step "capture backend discovery" write_discovery_json "$DISCOVERY_JSON" "$SCREEN_PNG"
run_step "summarize discovery" summarize_discovery "$DISCOVERY_JSON"
run_step "file capture probe" write_probe_json file "$FILE_PROBE_JSON" "$SCREEN_PNG"
run_step "validate file capture probe" assert_probe_ok "$FILE_PROBE_JSON" file
run_step "frame capture probe" write_probe_json frame "$FRAME_PROBE_JSON" "$SCREEN_PNG"
run_step "validate frame capture probe" assert_probe_ok "$FRAME_PROBE_JSON" frame
run_step "region capture probe" write_probe_json region "$REGION_PROBE_JSON" "$SCREEN_PNG"
run_step "validate region capture probe" assert_probe_ok "$REGION_PROBE_JSON" region

if should_probe_dmabuf "$DISCOVERY_JSON"; then
  run_step "DMA-BUF capture probe" write_probe_json dmabuf "$DMABUF_PROBE_JSON" "$SCREEN_PNG"
  run_step "validate DMA-BUF capture probe" assert_probe_ok "$DMABUF_PROBE_JSON" dmabuf
else
  echo "DMA-BUF probe skipped; unavailable or build lacks pipewire-backend." >"$DMABUF_PROBE_JSON"
  echo
  echo "== DMA-BUF capture probe =="
  cat "$DMABUF_PROBE_JSON"
fi

run_step "start observe daemon" start_daemon "$SOCKET" "$AUDIT_LOG"
if [[ -S "$SOCKET" ]]; then
  run_step "daemon capture backend discovery" \
    write_daemon_discovery_json "$SOCKET" "$DAEMON_JSON" "$DAEMON_SCREEN_PNG"
  run_step "summarize daemon discovery" summarize_discovery "$DAEMON_JSON"
fi

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX capture backend diagnostics example passed."
fi
