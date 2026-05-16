#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_ID="${PEEKABOOX_MCP_CAPTURE_BACKENDS_RUN_ID:-$(date +%Y%m%d-%H%M%S)}"
OUT_ROOT="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/mcp-capture-backends}"
OUT_DIR="$OUT_ROOT/$RUN_ID"
STRICT="${PEEKABOOX_STRICT:-0}"
INSTALL_PY_DEPS="${PEEKABOOX_MCP_CAPTURE_BACKENDS_INSTALL_PY_DEPS:-1}"
REGION="${PEEKABOOX_MCP_CAPTURE_BACKENDS_REGION:-0,0,320,180}"
SOCKET="$OUT_DIR/peekabooxd.sock"
AUDIT_LOG="$OUT_DIR/peekabooxd-audit.jsonl"
DAEMON_LOG="$OUT_DIR/peekabooxd.log"
PY_RUNTIME_DIR="$OUT_DIR/python-runtime"
GRPC_ADDR="${PEEKABOOX_MCP_CAPTURE_BACKENDS_GRPC_ADDR:-}"
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
    echo "Python runtime dependencies are missing; set PEEKABOOX_MCP_CAPTURE_BACKENDS_INSTALL_PY_DEPS=1 to create a local venv" >&2
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

call_capture_backends() {
  local response="$1"
  local request_id="$2"
  local output="$3"
  local probe="$4"
  local region="$5"

  PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
    "$PYTHON_RUNTIME" - "$request_id" "$output" "$probe" "$region" <<'PY' | run_mcp >"$response"
import json
import sys

request_id, output, probe, region = sys.argv[1:5]
arguments = {
    "output": output,
    "diagnose": True,
    "probe": probe,
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
            "params": {"name": "capture_backends", "arguments": arguments},
        }
    )
)
PY
}

validate_capture_backends() {
  local response="$1"
  local expected_probe="$2"
  local expected_region="$3"

  "$PYTHON_RUNTIME" - "$response" "$expected_probe" "$expected_region" <<'PY'
import json
import sys

path, expected_probe, expected_region = sys.argv[1:4]
payload = json.load(open(path, encoding="utf-8"))
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
result = payload.get("result", {})
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
data = result.get("structuredContent", {})

usable = [
    backend["name"]
    for backend in data.get("image_backends", [])
    if backend.get("available") and backend.get("supports_output")
]
if not usable:
    raise SystemExit("no usable output-capable image backend reported")
if not data.get("output_path"):
    raise SystemExit("output_path is missing")

if expected_region == "-":
    if data.get("region") is not None:
        raise SystemExit(f"unexpected region: {data.get('region')}")
else:
    x, y, width, height = [int(part) for part in expected_region.replace("x", ",").split(",")]
    expected = {"x": x, "y": y, "width": width, "height": height}
    if data.get("region") != expected:
        raise SystemExit(f"region mismatch: {data.get('region')} != {expected}")

if expected_probe != "none":
    probes = [probe for probe in data.get("probes", []) if probe.get("probe") == expected_probe]
    if not probes:
        raise SystemExit(f"missing probe result: {expected_probe}")
    probe = probes[0]
    if not probe.get("ok"):
        raise SystemExit(f"{expected_probe} probe failed: {probe.get('detail')}")
    if expected_probe == "file" and not probe.get("output_path"):
        raise SystemExit("file probe did not report output_path")
    if expected_probe == "region":
        width = expected["width"]
        height = expected["height"]
        if probe.get("width") != width or probe.get("height") != height:
            raise SystemExit(
                f"region probe size mismatch: {probe.get('width')}x{probe.get('height')}"
            )

print(
    "session={session} desktop={desktop} usable={usable} probe={probe} region={region}".format(
        session=data.get("session_type", "unknown"),
        desktop=data.get("desktop") or "-",
        usable=",".join(usable),
        probe=expected_probe,
        region=expected_region,
    )
)
PY
}

if [[ -e "$OUT_DIR" ]]; then
  echo "output directory already exists: $OUT_DIR" >&2
  exit 1
fi
mkdir -p "$OUT_DIR"

DISCOVERY_JSON="$OUT_DIR/backends.json"
FILE_PROBE_JSON="$OUT_DIR/probe-file.json"
FRAME_PROBE_JSON="$OUT_DIR/probe-frame.json"
REGION_PROBE_JSON="$OUT_DIR/probe-region.json"

echo "PeekabooX MCP capture-backends JSON-RPC output: $OUT_DIR"
echo "Probe region: $REGION"

run_step "ensure Python MCP runtime" ensure_python_runtime
run_step "start observe daemon" start_daemon

run_step "MCP capture backend discovery" \
  call_capture_backends "$DISCOVERY_JSON" 1 "$OUT_DIR/mcp-screen.png" none -
run_step "validate MCP capture backend discovery" \
  validate_capture_backends "$DISCOVERY_JSON" none -

run_step "MCP file capture backend probe" \
  call_capture_backends "$FILE_PROBE_JSON" 2 "$OUT_DIR/probe-file.png" file -
run_step "validate MCP file capture backend probe" \
  validate_capture_backends "$FILE_PROBE_JSON" file -

run_step "MCP frame capture backend probe" \
  call_capture_backends "$FRAME_PROBE_JSON" 3 "$OUT_DIR/probe-frame.png" frame -
run_step "validate MCP frame capture backend probe" \
  validate_capture_backends "$FRAME_PROBE_JSON" frame -

run_step "MCP region capture backend probe" \
  call_capture_backends "$REGION_PROBE_JSON" 4 "$OUT_DIR/probe-region.png" region "$REGION"
run_step "validate MCP region capture backend probe" \
  validate_capture_backends "$REGION_PROBE_JSON" region "$REGION"

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX MCP capture-backends JSON-RPC example passed."
fi
