#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_ID="${PEEKABOOX_MCP_CAPTURE_DELTA_RUN_ID:-$(date +%Y%m%d-%H%M%S)}"
OUT_ROOT="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/mcp-capture-delta}"
OUT_DIR="$OUT_ROOT/$RUN_ID"
STRICT="${PEEKABOOX_STRICT:-0}"
INSTALL_PY_DEPS="${PEEKABOOX_MCP_CAPTURE_DELTA_INSTALL_PY_DEPS:-1}"
REGION="${PEEKABOOX_MCP_CAPTURE_DELTA_REGION:-0,0,320,180}"
PRIMARY_STREAM="mcp-jsonrpc-$RUN_ID-primary"
REGION_STREAM="mcp-jsonrpc-$RUN_ID-region"
SOCKET="$OUT_DIR/peekabooxd.sock"
AUDIT_LOG="$OUT_DIR/peekabooxd-audit.jsonl"
DAEMON_LOG="$OUT_DIR/peekabooxd.log"
PY_RUNTIME_DIR="$OUT_DIR/python-runtime"
GRPC_ADDR="${PEEKABOOX_MCP_CAPTURE_DELTA_GRPC_ADDR:-}"
DAEMON_PID=""
PYTHON_RUNTIME=""
failures=0

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

pick_free_grpc_addr() {
  python3 - <<'PY'
import socket

sock = socket.socket()
sock.bind(("127.0.0.1", 0))
host, port = sock.getsockname()
sock.close()
print(f"{host}:{port}")
PY
}

python_has_runtime_deps() {
  local python_bin="$1"
  PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
    "$python_bin" - <<'PY' >/dev/null 2>&1
import grpc
import google.protobuf
import peekaboox
PY
}

ensure_python_runtime() {
  if [[ -n "${PEEKABOOX_PYTHON_BIN:-}" ]]; then
    if python_has_runtime_deps "$PEEKABOOX_PYTHON_BIN"; then
      PYTHON_RUNTIME="$PEEKABOOX_PYTHON_BIN"
      echo "python runtime: $PYTHON_RUNTIME"
      return 0
    fi
    echo "PEEKABOOX_PYTHON_BIN cannot import grpc/protobuf/peekaboox" >&2
    return 1
  fi

  if python_has_runtime_deps python3; then
    PYTHON_RUNTIME="python3"
    echo "python runtime: $PYTHON_RUNTIME"
    return 0
  fi

  if [[ "$INSTALL_PY_DEPS" != "1" ]]; then
    echo "Python runtime dependencies are missing; set PEEKABOOX_MCP_CAPTURE_DELTA_INSTALL_PY_DEPS=1 to create a local venv" >&2
    return 1
  fi

  python3 -m venv "$PY_RUNTIME_DIR"
  "$PY_RUNTIME_DIR/bin/python" -m pip install --upgrade pip
  "$PY_RUNTIME_DIR/bin/python" -m pip install -e "$ROOT/python"
  PYTHON_RUNTIME="$PY_RUNTIME_DIR/bin/python"
  echo "python runtime: $PYTHON_RUNTIME"
}

daemon_command() {
  if [[ -n "${PEEKABOOXD_BIN:-}" ]]; then
    printf '%s\0' "$PEEKABOOXD_BIN"
  elif command -v cargo >/dev/null 2>&1; then
    printf '%s\0' "cargo" "run" "--quiet" "-p" "peekabooxd" "--"
  else
    printf '%s\0' "peekabooxd"
  fi
}

wait_for_socket() {
  for _ in $(seq 1 80); do
    if [[ -S "$SOCKET" ]]; then
      return 0
    fi
    if [[ -n "$DAEMON_PID" ]] && ! kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
      echo "peekabooxd exited before creating socket" >&2
      sed -n '1,160p' "$DAEMON_LOG" >&2 || true
      return 1
    fi
    sleep 0.1
  done
  echo "timed out waiting for daemon socket: $SOCKET" >&2
  sed -n '1,160p' "$DAEMON_LOG" >&2 || true
  return 1
}

wait_for_grpc() {
  python3 - "$GRPC_ADDR" "$DAEMON_PID" "$DAEMON_LOG" <<'PY'
import os
import socket
import sys
import time

target, pid, log_path = sys.argv[1:4]
host, port_text = target.rsplit(":", 1)
port = int(port_text)
for _ in range(80):
    try:
        with socket.create_connection((host, port), timeout=0.2):
            raise SystemExit(0)
    except OSError:
        if pid and not os.path.exists(f"/proc/{pid}"):
            print("peekabooxd exited before opening gRPC", file=sys.stderr)
            try:
                print(open(log_path, encoding="utf-8").read(), file=sys.stderr)
            except OSError:
                pass
            raise SystemExit(1)
        time.sleep(0.1)
print(f"timed out waiting for gRPC: {target}", file=sys.stderr)
try:
    print(open(log_path, encoding="utf-8").read(), file=sys.stderr)
except OSError:
    pass
raise SystemExit(1)
PY
}

start_daemon() {
  if [[ -z "$GRPC_ADDR" ]]; then
    GRPC_ADDR="$(pick_free_grpc_addr)"
  fi

  local -a command
  mapfile -d '' -t command < <(daemon_command)
  "${command[@]}" run \
    --profile observe \
    --socket "$SOCKET" \
    --grpc-addr "$GRPC_ADDR" \
    --audit-log "$AUDIT_LOG" \
    --no-emergency-hotkey >"$DAEMON_LOG" 2>&1 &
  DAEMON_PID="$!"

  wait_for_socket
  wait_for_grpc
  echo "daemon pid: $DAEMON_PID"
  echo "daemon gRPC: $GRPC_ADDR"
}

run_mcp() {
  if [[ -n "${PEEKABOOX_MCP_BIN:-}" ]]; then
    "$PEEKABOOX_MCP_BIN" \
      --target "$GRPC_ADDR" \
      --capability-profile observe \
      --plugin-path "$ROOT/examples/plugins"
  else
    PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
      "$PYTHON_RUNTIME" -m peekaboox.mcp.server \
      --target "$GRPC_ADDR" \
      --capability-profile observe \
      --plugin-path "$ROOT/examples/plugins"
  fi
}

call_capture_delta() {
  local output="$1"
  local request_id="$2"
  local stream="$3"
  local reset="$4"
  local low_bandwidth="$5"
  local region="$6"

  PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
    "$PYTHON_RUNTIME" - "$request_id" "$stream" "$reset" "$low_bandwidth" "$region" <<'PY' | run_mcp >"$output"
import json
import sys

request_id, stream, reset, low_bandwidth, region = sys.argv[1:6]
arguments = {
    "stream_id": stream,
    "reset": reset == "true",
    "low_bandwidth": low_bandwidth == "true",
}
if region != "-":
    x, y, width, height = [int(part) for part in region.replace("x", ",").split(",")]
    arguments["region"] = {"x": x, "y": y, "width": width, "height": height}
print(
    json.dumps(
        {
            "jsonrpc": "2.0",
            "id": int(request_id),
            "method": "tools/call",
            "params": {"name": "capture_delta", "arguments": arguments},
        }
    )
)
PY
}

validate_capture_delta() {
  local output="$1"
  local stream="$2"
  local sequence="$3"
  local full_frame="$4"
  local low_bandwidth="$5"
  local region="$6"

  "$PYTHON_RUNTIME" - "$output" "$stream" "$sequence" "$full_frame" "$low_bandwidth" "$region" <<'PY'
import base64
import json
import sys

path, stream, sequence, full_frame, low_bandwidth, region_text = sys.argv[1:7]
payload = json.load(open(path, encoding="utf-8"))
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
result = payload.get("result", {})
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
data = result.get("structuredContent", {})
expected_sequence = int(sequence)
expected_full_frame = full_frame == "true"
expected_low_bandwidth = low_bandwidth == "true"

if data.get("stream_id") != stream:
    raise SystemExit(f"stream_id mismatch: {data.get('stream_id')} != {stream}")
if data.get("sequence") != expected_sequence:
    raise SystemExit(f"sequence mismatch: {data.get('sequence')} != {expected_sequence}")
if data.get("full_frame") is not expected_full_frame:
    raise SystemExit(f"full_frame mismatch: {data.get('full_frame')} != {expected_full_frame}")
if data.get("low_bandwidth") is not expected_low_bandwidth:
    raise SystemExit(
        f"low_bandwidth mismatch: {data.get('low_bandwidth')} != {expected_low_bandwidth}"
    )
if data.get("frame_width", 0) <= 0 or data.get("frame_height", 0) <= 0:
    raise SystemExit("frame dimensions must be positive")
patch_base64 = data.get("patch_base64", "")
base64.b64decode(patch_base64, validate=True)
if expected_full_frame and not patch_base64:
    raise SystemExit("full-frame capture delta must include patch_base64")

region = data.get("capture_region")
if region_text == "-":
    if region is not None:
        raise SystemExit(f"unexpected capture_region: {region}")
else:
    x, y, width, height = [int(part) for part in region_text.replace("x", ",").split(",")]
    expected = {"x": x, "y": y, "width": width, "height": height}
    if region != expected:
        raise SystemExit(f"capture_region mismatch: {region} != {expected}")
    if data.get("frame_width") != width or data.get("frame_height") != height:
        raise SystemExit(
            f"region frame size mismatch: {data.get('frame_width')}x{data.get('frame_height')}"
        )

print(
    "stream={stream} sequence={sequence} full_frame={full_frame} low_bandwidth={low_bandwidth} "
    "frame={width}x{height} changed_pixels={changed_pixels}".format(
        stream=data["stream_id"],
        sequence=data["sequence"],
        full_frame=data["full_frame"],
        low_bandwidth=data["low_bandwidth"],
        width=data["frame_width"],
        height=data["frame_height"],
        changed_pixels=data["changed_pixels"],
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
PRIMARY_FORCED_JSON="$OUT_DIR/primary-forced-full.json"
REGION_RESET_JSON="$OUT_DIR/region-reset.json"
REGION_DELTA_JSON="$OUT_DIR/region-delta.json"

echo "PeekabooX MCP capture-delta JSON-RPC output: $OUT_DIR"
echo "Primary stream: $PRIMARY_STREAM"
echo "Region stream: $REGION_STREAM"
echo "Region: $REGION"

run_step "ensure Python MCP runtime" ensure_python_runtime
run_step "start observe daemon" start_daemon

run_step "MCP primary reset full-frame capture" \
  call_capture_delta "$PRIMARY_RESET_JSON" 1 "$PRIMARY_STREAM" true true -
run_step "validate MCP primary reset full-frame capture" \
  validate_capture_delta "$PRIMARY_RESET_JSON" "$PRIMARY_STREAM" 1 true true -

run_step "MCP primary follow-up low-bandwidth delta" \
  call_capture_delta "$PRIMARY_DELTA_JSON" 2 "$PRIMARY_STREAM" false true -
run_step "validate MCP primary follow-up low-bandwidth delta" \
  validate_capture_delta "$PRIMARY_DELTA_JSON" "$PRIMARY_STREAM" 2 false true -

run_step "MCP primary forced full-frame capture" \
  call_capture_delta "$PRIMARY_FORCED_JSON" 3 "$PRIMARY_STREAM" false false -
run_step "validate MCP primary forced full-frame capture" \
  validate_capture_delta "$PRIMARY_FORCED_JSON" "$PRIMARY_STREAM" 3 true false -

run_step "MCP region reset full-frame capture" \
  call_capture_delta "$REGION_RESET_JSON" 4 "$REGION_STREAM" true true "$REGION"
run_step "validate MCP region reset full-frame capture" \
  validate_capture_delta "$REGION_RESET_JSON" "$REGION_STREAM" 1 true true "$REGION"

run_step "MCP region follow-up low-bandwidth delta" \
  call_capture_delta "$REGION_DELTA_JSON" 5 "$REGION_STREAM" false true "$REGION"
run_step "validate MCP region follow-up low-bandwidth delta" \
  validate_capture_delta "$REGION_DELTA_JSON" "$REGION_STREAM" 2 false true "$REGION"

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX MCP capture-delta JSON-RPC example passed."
fi
