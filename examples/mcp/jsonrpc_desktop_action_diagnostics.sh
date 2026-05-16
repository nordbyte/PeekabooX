#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ -n "${PEEKABOOX_PYTHON_BIN:-}" ]]; then
  PYTHON_RUNTIME="$PEEKABOOX_PYTHON_BIN"
elif [[ -x "$ROOT/.venv/bin/python" ]]; then
  PYTHON_RUNTIME="$ROOT/.venv/bin/python"
else
  PYTHON_RUNTIME="python3"
fi

APP="${PEEKABOOX_MCP_DESKTOP_ACTIONS_APP:-text-editor}"
TARGET="${PEEKABOOX_MCP_DESKTOP_ACTIONS_TARGET:-document}"
RUN_ID="${PEEKABOOX_MCP_DESKTOP_ACTIONS_RUN_ID:-$(date +%Y%m%d-%H%M%S)}"
OUT_ROOT="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/mcp-desktop-actions}"
OUT_DIR="$OUT_ROOT/$RUN_ID"
GRPC_ADDR="${PEEKABOOX_MCP_DESKTOP_ACTIONS_GRPC_ADDR:-}"
GRPC_TIMEOUT="${PEEKABOOX_MCP_DESKTOP_ACTIONS_GRPC_TIMEOUT:-20}"
WAIT_MS="${PEEKABOOX_MCP_DESKTOP_ACTIONS_WAIT_MS:-500}"
OVERVIEW_WAIT_MS="${PEEKABOOX_MCP_DESKTOP_ACTIONS_OVERVIEW_WAIT_MS:-1000}"
TYPE_VERIFY="${PEEKABOOX_MCP_DESKTOP_ACTIONS_TYPE_VERIFY:-0}"
START_DAEMON="${PEEKABOOX_MCP_DESKTOP_ACTIONS_START_DAEMON:-1}"
LAUNCH_EDITOR="${PEEKABOOX_MCP_DESKTOP_ACTIONS_LAUNCH_EDITOR:-1}"
TEXT="${PEEKABOOX_MCP_DESKTOP_ACTIONS_TEXT:-PeekabooX MCP desktop action diagnostics $RUN_ID}"
DAEMON_PID=""

run_mcp() {
  if [[ -n "${PEEKABOOX_MCP_BIN:-}" ]]; then
    "$PEEKABOOX_MCP_BIN" --plugin-path "$ROOT/examples/plugins" "$@"
  elif command -v peekaboox-mcp >/dev/null 2>&1; then
    peekaboox-mcp --plugin-path "$ROOT/examples/plugins" "$@"
  else
    PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
      "$PYTHON_RUNTIME" -m peekaboox.mcp.server --plugin-path "$ROOT/examples/plugins" "$@"
  fi
}

cleanup() {
  if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

pick_free_grpc_addr() {
  "$PYTHON_RUNTIME" - <<'PY'
import socket

sock = socket.socket()
sock.bind(("127.0.0.1", 0))
host, port = sock.getsockname()
sock.close()
print(f"{host}:{port}")
PY
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
  local socket_path="$1"
  local log_path="$2"
  for _ in $(seq 1 80); do
    if [[ -S "$socket_path" ]]; then
      return 0
    fi
    if [[ -n "$DAEMON_PID" ]] && ! kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
      echo "peekabooxd exited before creating socket" >&2
      sed -n '1,160p' "$log_path" >&2 || true
      return 1
    fi
    sleep 0.1
  done
  echo "timed out waiting for daemon socket: $socket_path" >&2
  sed -n '1,160p' "$log_path" >&2 || true
  return 1
}

wait_for_grpc() {
  local grpc_addr="$1"
  local log_path="$2"
  "$PYTHON_RUNTIME" - "$grpc_addr" "$log_path" <<'PY'
import socket
import sys
import time
from pathlib import Path

target, log_path = sys.argv[1:3]
host, port_text = target.rsplit(":", 1)
port = int(port_text)
for _ in range(80):
    try:
        with socket.create_connection((host, port), timeout=0.2):
            raise SystemExit(0)
    except OSError:
        time.sleep(0.1)
log = Path(log_path)
if log.exists():
    print(log.read_text(encoding="utf-8")[:4000], file=sys.stderr)
raise SystemExit(f"timed out waiting for gRPC: {target}")
PY
}

start_daemon() {
  mkdir -p "$OUT_DIR"
  local socket_path="$OUT_DIR/peekabooxd.sock"
  local audit_log="$OUT_DIR/peekabooxd-audit.jsonl"
  local daemon_log="$OUT_DIR/peekabooxd.log"
  if [[ -z "$GRPC_ADDR" ]]; then
    GRPC_ADDR="$(pick_free_grpc_addr)"
  fi

  local daemon_parts=()
  while IFS= read -r -d '' part; do
    daemon_parts+=("$part")
  done < <(daemon_command)

  "${daemon_parts[@]}" run \
    --profile operator \
    --socket "$socket_path" \
    --grpc-addr "$GRPC_ADDR" \
    --audit-log "$audit_log" \
    --no-emergency-hotkey \
    >"$daemon_log" 2>&1 &
  DAEMON_PID="$!"
  wait_for_socket "$socket_path" "$daemon_log"
  wait_for_grpc "$GRPC_ADDR" "$daemon_log"
}

launch_editor() {
  if [[ "$LAUNCH_EDITOR" != "1" ]]; then
    return 0
  fi
  local editor_bin="${PEEKABOOX_MCP_DESKTOP_ACTIONS_TEXT_EDITOR_BIN:-}"
  if [[ -z "$editor_bin" ]]; then
    if command -v gnome-text-editor >/dev/null 2>&1; then
      editor_bin="$(command -v gnome-text-editor)"
    elif command -v gedit >/dev/null 2>&1; then
      editor_bin="$(command -v gedit)"
    else
      echo "GNOME Text Editor is unavailable; set PEEKABOOX_MCP_DESKTOP_ACTIONS_TEXT_EDITOR_BIN" >&2
      return 1
    fi
  fi
  "$editor_bin" "$DRAFT_FILE" >/dev/null 2>&1 &
}

TOOLS_REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

printf '%s\n' "$TOOLS_REQUEST" | run_mcp | "$PYTHON_RUNTIME" -c '
import json
import sys

payload = json.load(sys.stdin)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
tools = {tool["name"]: tool for tool in payload["result"]["tools"]}
expected_inputs = {
    "desktop_click": {
        "app", "target", "image_path", "prefer_accessibility", "window_title",
        "window_id", "button", "dry_run", "verify",
    },
    "desktop_drag": {
        "app", "target", "image_path", "prefer_accessibility", "window_title",
        "window_id", "button", "from_ratio", "to_ratio", "duration_ms",
        "dry_run", "verify",
    },
    "desktop_type_into": {
        "app", "target", "text", "image_path", "prefer_accessibility",
        "window_title", "window_id", "clear", "dry_run", "verify",
    },
}
for name, inputs in expected_inputs.items():
    tool = tools.get(name)
    if tool is None:
        raise SystemExit(f"{name} tool missing")
    input_properties = tool.get("inputSchema", {}).get("properties", {})
    missing_input = sorted(inputs - set(input_properties))
    if missing_input:
        raise SystemExit(f"{name} input schema missing: {missing_input}")
    output_schema = tool.get("outputSchema", {})
    output_properties = output_schema.get("properties", {})
    if "focus_diagnostics" not in output_properties:
        raise SystemExit(f"{name} outputSchema missing focus_diagnostics")
    if "focus_diagnostics" not in output_schema.get("required", []):
        raise SystemExit(f"{name} outputSchema should require focus_diagnostics")
    focus_schema = output_properties["focus_diagnostics"]
    if focus_schema.get("type") != "array" or focus_schema.get("items", {}).get("type") != "string":
        raise SystemExit(f"{name} focus_diagnostics output schema must be string[]")
print(json.dumps({"tools": sorted(expected_inputs), "schema": "focus_diagnostics"}, sort_keys=True))
'

if [[ "${PEEKABOOX_MCP_DESKTOP_ACTIONS_LIVE:-0}" != "1" ]]; then
  echo "Schema check completed. Set PEEKABOOX_MCP_DESKTOP_ACTIONS_LIVE=1 to call a live daemon through MCP."
  echo "Live mode opens a unique Text Editor draft under target/examples and leaves the editor window open for inspection."
  exit 0
fi

if [[ -e "$OUT_DIR" ]]; then
  echo "output directory already exists: $OUT_DIR" >&2
  exit 1
fi
mkdir -p "$OUT_DIR"
DRAFT_FILE="$OUT_DIR/peekaboox-mcp-desktop-actions-$RUN_ID.txt"
WINDOW_ID="${PEEKABOOX_MCP_DESKTOP_ACTIONS_WINDOW_ID:-}"
if [[ -n "${PEEKABOOX_MCP_DESKTOP_ACTIONS_WINDOW_TITLE:-}" ]]; then
  WINDOW_TITLE="$PEEKABOOX_MCP_DESKTOP_ACTIONS_WINDOW_TITLE"
elif [[ -z "$WINDOW_ID" ]]; then
  WINDOW_TITLE="$(basename "$DRAFT_FILE")"
else
  WINDOW_TITLE=""
fi
if [[ -n "$WINDOW_TITLE" && -n "$WINDOW_ID" ]]; then
  echo "set either PEEKABOOX_MCP_DESKTOP_ACTIONS_WINDOW_TITLE or WINDOW_ID, not both" >&2
  exit 1
fi
printf 'PeekabooX MCP desktop action diagnostics draft\n' >"$DRAFT_FILE"

if [[ "$START_DAEMON" == "1" ]]; then
  start_daemon
elif [[ -z "$GRPC_ADDR" ]]; then
  GRPC_ADDR="127.0.0.1:47777"
fi
launch_editor

build_request() {
  local request_id="$1"
  local tool="$2"
  "$PYTHON_RUNTIME" - \
    "$request_id" "$tool" "$APP" "$TARGET" "$WINDOW_TITLE" "$WINDOW_ID" "$TEXT" "$TYPE_VERIFY" "$WAIT_MS" "$OVERVIEW_WAIT_MS" <<'PY'
import json
import sys

(
    request_id,
    tool,
    app,
    target,
    window_title,
    window_id,
    text,
    type_verify,
    wait_ms,
    overview_wait_ms,
) = sys.argv[1:11]
arguments = {"app": app}
if tool != "desktop_focus":
    arguments["target"] = target
if window_title:
    arguments["window_title"] = window_title
if window_id:
    arguments["window_id"] = window_id

if tool == "desktop_focus":
    arguments.update({
        "verify": True,
        "wait_after_focus_ms": int(wait_ms),
        "overview_wait_ms": int(overview_wait_ms),
    })
elif tool == "desktop_click":
    arguments.update({"button": "left", "verify": True})
elif tool == "desktop_drag":
    arguments.update({
        "from_ratio": [0.2, 0.5],
        "to_ratio": [0.8, 0.5],
        "duration_ms": 120,
        "verify": True,
    })
elif tool == "desktop_type_into":
    arguments.update({
        "text": text,
        "clear": True,
        "verify": type_verify.strip().lower() in {"1", "true", "yes", "on"},
    })
else:
    raise SystemExit(f"unsupported tool: {tool}")

print(json.dumps({
    "jsonrpc": "2.0",
    "id": int(request_id),
    "method": "tools/call",
    "params": {"name": tool, "arguments": arguments},
}, separators=(",", ":")))
PY
}

validate_response() {
  local response="$1"
  local tool="$2"
  local expected_action="$3"
  "$PYTHON_RUNTIME" - "$response" "$tool" "$expected_action" <<'PY'
import json
import sys
from pathlib import Path

response_path, tool, expected_action = sys.argv[1:4]
payload = json.loads(Path(response_path).read_text(encoding="utf-8"))
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
result = payload.get("result", {})
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
structured = result.get("structuredContent", {})
if structured.get("action") != expected_action:
    raise SystemExit(f"{tool} action mismatch: {structured.get('action')!r}")
diagnostics = structured.get("focus_diagnostics")
if not isinstance(diagnostics, list) or not diagnostics:
    raise SystemExit(f"{tool} returned empty focus_diagnostics")
if not all(isinstance(item, str) and item for item in diagnostics):
    raise SystemExit(f"{tool} focus_diagnostics must contain non-empty strings")
if not any(item.startswith("verify:") for item in diagnostics):
    raise SystemExit(f"{tool} should include a focus verify diagnostic")
text_blocks = [
    item for item in result.get("content", [])
    if item.get("type") == "text" and isinstance(item.get("text"), str)
]
if not text_blocks:
    raise SystemExit(f"{tool} missing MCP text content compatibility block")
text_payload = json.loads(text_blocks[0]["text"])
if text_payload.get("focus_diagnostics") != diagnostics:
    raise SystemExit(f"{tool} text content focus_diagnostics differs from structuredContent")
print(json.dumps({
    "tool": tool,
    "action": structured.get("action"),
    "backend_name": structured.get("backend_name"),
    "verified": structured.get("verified"),
    "diagnostic_count": len(diagnostics),
    "last_diagnostic": diagnostics[-1],
}, sort_keys=True))
PY
}

call_tool() {
  local request_id="$1"
  local tool="$2"
  local expected_action="$3"
  local response="$OUT_DIR/$tool.json"
  build_request "$request_id" "$tool" \
    | run_mcp --target "$GRPC_ADDR" --capability-profile operator --grpc-timeout "$GRPC_TIMEOUT" \
    >"$response"
  validate_response "$response" "$tool" "$expected_action"
}

wait_for_focus() {
  local response="$OUT_DIR/desktop_focus.json"
  local last_error="$OUT_DIR/desktop_focus.last-error.json"
  for _ in $(seq 1 24); do
    if build_request 10 desktop_focus \
      | run_mcp --target "$GRPC_ADDR" --capability-profile operator --grpc-timeout "$GRPC_TIMEOUT" \
      >"$response"; then
      if "$PYTHON_RUNTIME" - "$response" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
raise SystemExit(0 if not payload.get("result", {}).get("isError") and "error" not in payload else 1)
PY
      then
        return 0
      fi
    fi
    cp "$response" "$last_error" 2>/dev/null || true
    sleep 0.25
  done
  echo "timed out waiting for focus target" >&2
  sed -n '1,120p' "$last_error" >&2 || true
  return 1
}

wait_for_focus
call_tool 20 desktop_click click
call_tool 21 desktop_drag drag
call_tool 22 desktop_type_into type-into

echo "PeekabooX MCP desktop action diagnostics JSON-RPC example passed."
