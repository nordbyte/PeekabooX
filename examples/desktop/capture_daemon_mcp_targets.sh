#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_ID="${PEEKABOOX_CAPTURE_PARITY_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
OUT_ROOT="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/capture-daemon-mcp-targets}"
OUT_DIR="$OUT_ROOT/$RUN_ID"
STRICT="${PEEKABOOX_STRICT:-0}"
APP_QUERY="${PEEKABOOX_CAPTURE_PARITY_APP_QUERY:-calculator}"
WINDOW_TITLE_REGEX="${PEEKABOOX_CAPTURE_PARITY_TITLE_REGEX:-Calculator}"
LAUNCH_DELAY="${PEEKABOOX_CAPTURE_PARITY_LAUNCH_DELAY:-2}"
REGION_X="${PEEKABOOX_CAPTURE_PARITY_REGION_X:-10}"
REGION_Y="${PEEKABOOX_CAPTURE_PARITY_REGION_Y:-10}"
REGION_WIDTH="${PEEKABOOX_CAPTURE_PARITY_REGION_WIDTH:-120}"
REGION_HEIGHT="${PEEKABOOX_CAPTURE_PARITY_REGION_HEIGHT:-80}"

SOCKET="${PEEKABOOX_CAPTURE_PARITY_SOCKET:-${XDG_RUNTIME_DIR:-/tmp}/px-cap-parity-$$.sock}"
AUDIT_LOG="$OUT_DIR/peekabooxd-audit.jsonl"
DAEMON_LOG="$OUT_DIR/peekabooxd.log"
PY_RUNTIME_DIR="$OUT_DIR/python-runtime"
GRPC_ADDR="${PEEKABOOX_CAPTURE_PARITY_GRPC_ADDR:-}"
DAEMON_PID=""
PYTHON_RUNTIME=""
launched_pid=""
failures=0

WINDOWS_JSON="$OUT_DIR/windows-$RUN_ID.json"
DAEMON_APP_JSON="$OUT_DIR/daemon-app-$RUN_ID.json"
DAEMON_APP_PNG="$OUT_DIR/daemon-app-$RUN_ID.png"
DAEMON_REGION_JSON="$OUT_DIR/daemon-region-$RUN_ID.json"
DAEMON_REGION_PNG="$OUT_DIR/daemon-region-$RUN_ID.png"
DAEMON_SEMANTIC_JSON="$OUT_DIR/daemon-semantic-$RUN_ID.json"
DAEMON_SEMANTIC_PNG="$OUT_DIR/daemon-semantic-$RUN_ID.png"
DAEMON_XWD_JSON="$OUT_DIR/daemon-xwd-$RUN_ID.json"
DAEMON_XWD="$OUT_DIR/daemon-xwd-$RUN_ID.xwd"
DAEMON_NO_OVERWRITE="$OUT_DIR/daemon-no-overwrite-$RUN_ID.png"
PYTHON_APP_JSON="$OUT_DIR/python-app-$RUN_ID.json"
PYTHON_APP_PNG="$OUT_DIR/python-app-$RUN_ID.png"
PYTHON_REGION_JSON="$OUT_DIR/python-region-$RUN_ID.json"
PYTHON_REGION_PNG="$OUT_DIR/python-region-$RUN_ID.png"
MCP_APP_JSON="$OUT_DIR/mcp-app-$RUN_ID.json"
MCP_REGION_JSON="$OUT_DIR/mcp-region-$RUN_ID.json"

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

daemon_command() {
  if [[ -n "${PEEKABOOXD_BIN:-}" ]]; then
    printf '%s\0' "$PEEKABOOXD_BIN"
  elif fresh_binary \
    "$ROOT/target/debug/peekabooxd" \
    "$ROOT/rust/daemon/src/main.rs" \
    "$ROOT/rust/capture/src/lib.rs" \
    "$ROOT/rust/ipc/src/lib.rs"; then
    printf '%s\0' "$ROOT/target/debug/peekabooxd"
  elif command -v cargo >/dev/null 2>&1; then
    printf '%s\0' "cargo" "run" "--quiet" "-p" "peekabooxd" "--"
  else
    printf '%s\0' "peekabooxd"
  fi
}

cleanup() {
  if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
  rm -f "$SOCKET"
  if [[ -n "$launched_pid" && "${PEEKABOOX_CAPTURE_PARITY_CLOSE_APP:-0}" == "1" ]]; then
    kill "$launched_pid" >/dev/null 2>&1 || true
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

skip_or_fail() {
  local message="$1"
  if [[ "$STRICT" == "1" ]]; then
    echo "error: $message" >&2
    exit 1
  fi
  echo "warning: $message" >&2
  exit 0
}

find_calculator_app() {
  if [[ -n "${PEEKABOOX_CAPTURE_PARITY_APP:-}" ]]; then
    printf '%s\n' "$PEEKABOOX_CAPTURE_PARITY_APP"
    return 0
  fi
  if command -v gnome-calculator >/dev/null 2>&1; then
    printf '%s\n' "gnome-calculator"
    return 0
  fi
  if command -v kcalc >/dev/null 2>&1; then
    printf '%s\n' "kcalc"
    return 0
  fi
  if command -v galculator >/dev/null 2>&1; then
    printf '%s\n' "galculator"
    return 0
  fi
  return 1
}

write_windows_json() {
  run_peekaboox windows --json --app "$APP_QUERY" --title-regex "$WINDOW_TITLE_REGEX" >"$WINDOWS_JSON"
}

first_window_id() {
  python3 - "$WINDOWS_JSON" <<'PY'
import json
import sys

payload = json.load(open(sys.argv[1], encoding="utf-8"))
windows = payload.get("windows") or []
if not windows:
    raise SystemExit(1)
print(windows[0]["id"])
PY
}

ensure_target_window() {
  write_windows_json && first_window_id >/dev/null && return 0

  local app
  app="$(find_calculator_app)" || skip_or_fail "no calculator app found; set PEEKABOOX_CAPTURE_PARITY_APP"
  "$app" >/dev/null 2>&1 &
  launched_pid="$!"
  sleep "$LAUNCH_DELAY"
  write_windows_json
  first_window_id >/dev/null
}

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
        try:
            os.kill(int(pid), 0)
        except OSError:
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
  rm -f "$SOCKET"
  "${command[@]}" run \
    --profile observe \
    --socket "$SOCKET" \
    --grpc-addr "$GRPC_ADDR" \
    --audit-log "$AUDIT_LOG" \
    --no-emergency-hotkey >"$DAEMON_LOG" 2>&1 &
  DAEMON_PID="$!"

  wait_for_socket || return 1
  wait_for_grpc || return 1
  echo "daemon pid: $DAEMON_PID"
  echo "daemon gRPC: $GRPC_ADDR"
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

  if [[ -x "$ROOT/.venv/bin/python" ]] && python_has_runtime_deps "$ROOT/.venv/bin/python"; then
    PYTHON_RUNTIME="$ROOT/.venv/bin/python"
    echo "python runtime: $PYTHON_RUNTIME"
    return 0
  fi

  if python_has_runtime_deps python3; then
    PYTHON_RUNTIME="python3"
    echo "python runtime: $PYTHON_RUNTIME"
    return 0
  fi

  python3 -m venv "$PY_RUNTIME_DIR"
  "$PY_RUNTIME_DIR/bin/python" -m pip install --upgrade pip
  "$PY_RUNTIME_DIR/bin/python" -m pip install -e "$ROOT/python"
  PYTHON_RUNTIME="$PY_RUNTIME_DIR/bin/python"
  echo "python runtime: $PYTHON_RUNTIME"
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

validate_daemon_capture_json() {
  local json_file="$1"
  local image_file="$2"
  local mime_type="$3"
  local expected_width="$4"
  local expected_height="$5"
  local require_window="$6"
  local allow_zero_size="$7"
  python3 - "$json_file" "$image_file" "$mime_type" "$expected_width" "$expected_height" "$require_window" "$allow_zero_size" <<'PY'
import json
import os
import sys

json_file, image_file, mime_type, width_text, height_text, require_window, allow_zero = sys.argv[1:8]
payload = json.load(open(json_file, encoding="utf-8"))
if payload.get("mime_type") != mime_type:
    raise SystemExit(f"unexpected mime type: {payload.get('mime_type')}")
if payload.get("bytes_written", 0) <= 0:
    raise SystemExit("capture reported no bytes")
if not os.path.exists(image_file) or os.path.getsize(image_file) <= 0:
    raise SystemExit(f"missing capture output: {image_file}")
if payload.get("output_path") and os.path.abspath(image_file) != payload["output_path"]:
    raise SystemExit("capture output_path does not match requested file")
if require_window == "1" and not payload.get("window_id"):
    raise SystemExit("window-target capture did not report window_id")
if allow_zero != "1" and (payload.get("width", 0) <= 0 or payload.get("height", 0) <= 0):
    raise SystemExit("capture reported empty dimensions")
if width_text != "-" and payload.get("width") != int(width_text):
    raise SystemExit(f"width mismatch: {payload.get('width')} != {width_text}")
if height_text != "-" and payload.get("height") != int(height_text):
    raise SystemExit(f"height mismatch: {payload.get('height')} != {height_text}")
region = payload.get("capture_region")
if width_text != "-" and region and region.get("width") != int(width_text):
    raise SystemExit(f"capture_region width mismatch: {region}")
if height_text != "-" and region and region.get("height") != int(height_text):
    raise SystemExit(f"capture_region height mismatch: {region}")
if "captured_at_unix_ms" not in payload or "source" not in payload:
    raise SystemExit("capture metadata is incomplete")
print(f"{mime_type} {payload.get('width')}x{payload.get('height')} via {payload.get('backend_name')} source={payload.get('source')}")
PY
}

daemon_capture_app() {
  run_peekaboox --daemon --socket "$SOCKET" capture \
    --json \
    --app "$APP_QUERY" \
    --title-regex "$WINDOW_TITLE_REGEX" \
    --output "$DAEMON_APP_PNG" >"$DAEMON_APP_JSON"
}

daemon_capture_region() {
  local window_id
  window_id="$(first_window_id)"
  run_peekaboox --daemon --socket "$SOCKET" capture \
    --json \
    --window-id "$window_id" \
    --region "$REGION_X,$REGION_Y,$REGION_WIDTH,$REGION_HEIGHT" \
    --output "$DAEMON_REGION_PNG" >"$DAEMON_REGION_JSON"
}

daemon_capture_semantic() {
  run_peekaboox --daemon --socket "$SOCKET" capture \
    --json \
    --include-semantic-tree \
    --app "$APP_QUERY" \
    --title-regex "$WINDOW_TITLE_REGEX" \
    --output "$DAEMON_SEMANTIC_PNG" >"$DAEMON_SEMANTIC_JSON"
}

validate_semantic_json() {
  python3 - "$DAEMON_SEMANTIC_JSON" <<'PY'
import json
import sys

payload = json.load(open(sys.argv[1], encoding="utf-8"))
semantic_tree = payload.get("semantic_tree")
if not isinstance(semantic_tree, list):
    raise SystemExit("semantic_tree is not a list")
print(f"semantic_tree elements: {len(semantic_tree)}")
PY
}

daemon_capture_xwd() {
  run_peekaboox --daemon --socket "$SOCKET" capture \
    --json \
    --format xwd \
    --output "$DAEMON_XWD" >"$DAEMON_XWD_JSON"
}

daemon_no_overwrite_rejects_existing_file() {
  printf 'existing\n' >"$DAEMON_NO_OVERWRITE"
  if run_peekaboox --daemon --socket "$SOCKET" capture --output "$DAEMON_NO_OVERWRITE" --no-overwrite >/dev/null 2>&1; then
    echo "daemon capture unexpectedly overwrote existing file" >&2
    return 1
  fi
}

python_runtime_capture_targets() {
  PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
    "$PYTHON_RUNTIME" - \
      "$GRPC_ADDR" \
      "$APP_QUERY" \
      "$WINDOW_TITLE_REGEX" \
      "$REGION_X" \
      "$REGION_Y" \
      "$REGION_WIDTH" \
      "$REGION_HEIGHT" \
      "$PYTHON_APP_JSON" \
      "$PYTHON_APP_PNG" \
      "$PYTHON_REGION_JSON" \
      "$PYTHON_REGION_PNG" <<'PY'
import base64
import json
import sys
from dataclasses import asdict

from peekaboox.agent import AgentRuntime
from peekaboox.client import Rect

(
    target,
    app,
    title_regex,
    region_x,
    region_y,
    region_width,
    region_height,
    app_json,
    app_png,
    region_json,
    region_png,
) = sys.argv[1:12]

runtime = AgentRuntime.connect(target=target, capability_profile="observe")

def write_capture(result, json_path, image_path):
    with open(image_path, "wb") as handle:
        handle.write(result.image)
    payload = {
        "mime_type": result.mime_type,
        "image_bytes": len(result.image),
        "metadata": asdict(result.metadata) if result.metadata is not None else None,
        "semantic_tree_count": len(result.semantic_tree),
    }
    with open(json_path, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")

write_capture(
    runtime.capture_screen(app=app, title_regex=title_regex),
    app_json,
    app_png,
)
write_capture(
    runtime.capture_screen(
        app=app,
        title_regex=title_regex,
        region=Rect(
            x=int(region_x),
            y=int(region_y),
            width=int(region_width),
            height=int(region_height),
        ),
    ),
    region_json,
    region_png,
)
PY
}

validate_runtime_capture_json() {
  local json_file="$1"
  local image_file="$2"
  local expected_width="$3"
  local expected_height="$4"
  "$PYTHON_RUNTIME" - "$json_file" "$image_file" "$expected_width" "$expected_height" <<'PY'
import json
import os
import sys

json_file, image_file, expected_width, expected_height = sys.argv[1:5]
payload = json.load(open(json_file, encoding="utf-8"))
if payload.get("mime_type") != "image/png":
    raise SystemExit(f"unexpected mime type: {payload.get('mime_type')}")
if payload.get("image_bytes", 0) <= 0:
    raise SystemExit("capture image was empty")
if not os.path.exists(image_file) or os.path.getsize(image_file) <= 0:
    raise SystemExit(f"missing runtime image: {image_file}")
metadata = payload.get("metadata") or {}
if expected_width != "-" and metadata.get("width") != int(expected_width):
    raise SystemExit(f"width mismatch: {metadata.get('width')} != {expected_width}")
if expected_height != "-" and metadata.get("height") != int(expected_height):
    raise SystemExit(f"height mismatch: {metadata.get('height')} != {expected_height}")
if metadata.get("width", 0) <= 0 or metadata.get("height", 0) <= 0:
    raise SystemExit("runtime metadata reported empty dimensions")
print(f"runtime {metadata.get('width')}x{metadata.get('height')} via {metadata.get('backend')}")
PY
}

call_mcp_capture() {
  local response="$1"
  local request_id="$2"
  local with_region="$3"

  PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
    "$PYTHON_RUNTIME" - \
      "$request_id" \
      "$APP_QUERY" \
      "$WINDOW_TITLE_REGEX" \
      "$with_region" \
      "$REGION_X" \
      "$REGION_Y" \
      "$REGION_WIDTH" \
      "$REGION_HEIGHT" <<'PY' | run_mcp >"$response"
import json
import sys

request_id, app, title_regex, with_region, region_x, region_y, region_width, region_height = sys.argv[1:9]
arguments = {
    "app": app,
    "title_regex": title_regex,
}
if with_region == "1":
    arguments["region"] = {
        "x": int(region_x),
        "y": int(region_y),
        "width": int(region_width),
        "height": int(region_height),
    }
print(
    json.dumps(
        {
            "jsonrpc": "2.0",
            "id": int(request_id),
            "method": "tools/call",
            "params": {"name": "capture_screen", "arguments": arguments},
        }
    )
)
PY
}

validate_mcp_capture() {
  local response="$1"
  local expected_width="$2"
  local expected_height="$3"
  "$PYTHON_RUNTIME" - "$response" "$expected_width" "$expected_height" <<'PY'
import base64
import json
import sys

path, expected_width, expected_height = sys.argv[1:4]
payload = json.load(open(path, encoding="utf-8"))
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
result = payload.get("result", {})
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
data = result.get("structuredContent", {})
if data.get("mime_type") != "image/png":
    raise SystemExit(f"unexpected MIME type: {data.get('mime_type')}")
image = base64.b64decode(data.get("image_base64", ""))
if not image:
    raise SystemExit("MCP capture image is empty")
metadata = data.get("metadata") or {}
if expected_width != "-" and metadata.get("width") != int(expected_width):
    raise SystemExit(f"width mismatch: {metadata.get('width')} != {expected_width}")
if expected_height != "-" and metadata.get("height") != int(expected_height):
    raise SystemExit(f"height mismatch: {metadata.get('height')} != {expected_height}")
if metadata.get("width", 0) <= 0 or metadata.get("height", 0) <= 0:
    raise SystemExit("MCP metadata reported empty dimensions")
content = result.get("content") or []
if not any(item.get("type") == "image" for item in content if isinstance(item, dict)):
    raise SystemExit("MCP response did not include image content")
print(f"MCP {metadata.get('width')}x{metadata.get('height')} image_bytes={len(image)}")
PY
}

if [[ -e "$OUT_DIR" ]]; then
  echo "output directory already exists: $OUT_DIR" >&2
  exit 1
fi
mkdir -p "$OUT_DIR"

echo "PeekabooX daemon/Python/MCP capture target output: $OUT_DIR"
echo "Window filter: app=$APP_QUERY title_regex=$WINDOW_TITLE_REGEX"
echo "Window-relative region: $REGION_X,$REGION_Y,$REGION_WIDTH,$REGION_HEIGHT"

run_step "ensure target window" ensure_target_window
run_step "ensure Python runtime" ensure_python_runtime
run_step "start observe daemon" start_daemon

run_step "daemon capture by app/title-regex" daemon_capture_app
run_step "validate daemon app/title capture" \
  validate_daemon_capture_json "$DAEMON_APP_JSON" "$DAEMON_APP_PNG" image/png - - 1 0
run_step "daemon capture window-relative region" daemon_capture_region
run_step "validate daemon region capture" \
  validate_daemon_capture_json "$DAEMON_REGION_JSON" "$DAEMON_REGION_PNG" image/png "$REGION_WIDTH" "$REGION_HEIGHT" 1 0
run_step "daemon capture semantic tree metadata" daemon_capture_semantic
run_step "validate daemon semantic capture" \
  validate_daemon_capture_json "$DAEMON_SEMANTIC_JSON" "$DAEMON_SEMANTIC_PNG" image/png - - 1 0
run_step "validate daemon semantic tree field" validate_semantic_json
run_step "daemon capture full screen as XWD" daemon_capture_xwd
run_step "validate daemon XWD capture" \
  validate_daemon_capture_json "$DAEMON_XWD_JSON" "$DAEMON_XWD" image/x-xwindowdump - - 0 1
run_step "daemon no-overwrite guard" daemon_no_overwrite_rejects_existing_file

run_step "Python runtime capture app and region targets" python_runtime_capture_targets
run_step "validate Python runtime app capture" \
  validate_runtime_capture_json "$PYTHON_APP_JSON" "$PYTHON_APP_PNG" - -
run_step "validate Python runtime region capture" \
  validate_runtime_capture_json "$PYTHON_REGION_JSON" "$PYTHON_REGION_PNG" "$REGION_WIDTH" "$REGION_HEIGHT"

run_step "MCP capture app/title-regex target" call_mcp_capture "$MCP_APP_JSON" 1 0
run_step "validate MCP app/title capture" validate_mcp_capture "$MCP_APP_JSON" - -
run_step "MCP capture window-relative region target" call_mcp_capture "$MCP_REGION_JSON" 2 1
run_step "validate MCP region capture" validate_mcp_capture "$MCP_REGION_JSON" "$REGION_WIDTH" "$REGION_HEIGHT"

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX daemon/Python/MCP capture target example passed."
fi
