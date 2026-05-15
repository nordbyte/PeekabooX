#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/windows-live-smoke}}"
RUN_ID="${PEEKABOOX_WINDOWS_LIVE_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
STRICT="${PEEKABOOX_STRICT:-0}"
WINDOWS_BACKEND="${PEEKABOOX_WINDOWS_LIVE_BACKEND:-auto}"
INSTALL_PY_DEPS="${PEEKABOOX_WINDOWS_LIVE_INSTALL_PY_DEPS:-1}"

DIRECT_ALL_JSON="$OUT_DIR/windows-direct-all-$RUN_ID.json"
DIRECT_FOCUSED_JSON="$OUT_DIR/windows-direct-focused-$RUN_ID.json"
DIRECT_APP_JSON="$OUT_DIR/windows-direct-app-$RUN_ID.json"
DIRECT_TITLE_JSON="$OUT_DIR/windows-direct-title-$RUN_ID.json"
DIRECT_REGEX_JSON="$OUT_DIR/windows-direct-regex-$RUN_ID.json"
DIRECT_ID_JSON="$OUT_DIR/windows-direct-id-$RUN_ID.json"
DAEMON_FOCUSED_JSON="$OUT_DIR/windows-daemon-focused-$RUN_ID.json"
DAEMON_FILTERED_JSON="$OUT_DIR/windows-daemon-filtered-$RUN_ID.json"
AGENT_FOCUSED_JSON="$OUT_DIR/windows-agent-focused-$RUN_ID.json"
AGENT_FILTERED_JSON="$OUT_DIR/windows-agent-filtered-$RUN_ID.json"
MCP_FILTERED_JSON="$OUT_DIR/windows-mcp-filtered-$RUN_ID.json"
QUERY_JSON="$OUT_DIR/windows-query-$RUN_ID.json"
DAEMON_LOG="$OUT_DIR/peekabooxd-$RUN_ID.log"
AUDIT_LOG="$OUT_DIR/peekabooxd-audit-$RUN_ID.jsonl"
SOCKET="${PEEKABOOX_WINDOWS_LIVE_SOCKET:-$OUT_DIR/peekabooxd-$RUN_ID.sock}"
GRPC_ADDR="${PEEKABOOX_WINDOWS_LIVE_GRPC_ADDR:-}"
PY_RUNTIME_DIR="$OUT_DIR/python-runtime"

daemon_pid=""
PYTHON_RUNTIME=""
failures=0

run_peekaboox() {
  if [[ -n "${PEEKABOOX_BIN:-}" ]]; then
    "$PEEKABOOX_BIN" "$@"
  elif [[ -x "$ROOT/target/debug/peekaboox" ]]; then
    "$ROOT/target/debug/peekaboox" "$@"
  elif command -v peekaboox >/dev/null 2>&1; then
    peekaboox "$@"
  else
    cargo run --quiet -p peekaboox-cli -- "$@"
  fi
}

daemon_command() {
  if [[ -n "${PEEKABOOXD_BIN:-}" ]]; then
    printf '%s\0' "$PEEKABOOXD_BIN"
  elif [[ -x "$ROOT/target/debug/peekabooxd" ]]; then
    printf '%s\0' "$ROOT/target/debug/peekabooxd"
  elif command -v peekabooxd >/dev/null 2>&1; then
    printf '%s\0' "peekabooxd"
  else
    printf '%s\0' "cargo" "run" "--quiet" "-p" "peekabooxd" "--"
  fi
}

run_step() {
  local description="$1"
  shift
  printf '\n== %s ==\n' "$description"
  if "$@"; then
    return 0
  fi

  failures=$((failures + 1))
  if [[ "$STRICT" == "1" ]]; then
    echo "failed: $description" >&2
    exit 1
  fi
  echo "warning: $description failed in this desktop environment" >&2
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

cleanup() {
  if [[ -n "$daemon_pid" ]] && kill -0 "$daemon_pid" >/dev/null 2>&1; then
    kill "$daemon_pid" >/dev/null 2>&1 || true
    wait "$daemon_pid" >/dev/null 2>&1 || true
  fi
  if [[ -S "$SOCKET" ]]; then
    rm -f "$SOCKET"
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

wait_for_socket() {
  local socket_path="$1"
  for _ in $(seq 1 80); do
    if [[ -S "$socket_path" ]]; then
      return 0
    fi
    if [[ -n "$daemon_pid" ]] && ! kill -0 "$daemon_pid" >/dev/null 2>&1; then
      echo "peekabooxd exited before creating socket" >&2
      sed -n '1,160p' "$DAEMON_LOG" >&2 || true
      return 1
    fi
    sleep 0.1
  done
  echo "timed out waiting for daemon socket: $socket_path" >&2
  sed -n '1,160p' "$DAEMON_LOG" >&2 || true
  return 1
}

wait_for_grpc() {
  python3 - "$1" "$daemon_pid" "$DAEMON_LOG" <<'PY'
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
  if [[ -e "$SOCKET" ]]; then
    echo "refusing to overwrite existing socket path: $SOCKET" >&2
    return 1
  fi
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
  daemon_pid="$!"

  wait_for_socket "$SOCKET"
  wait_for_grpc "$GRPC_ADDR"
  echo "daemon pid: $daemon_pid"
  echo "daemon socket: $SOCKET"
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

  if python_has_runtime_deps python3; then
    PYTHON_RUNTIME="python3"
    echo "python runtime: $PYTHON_RUNTIME"
    return 0
  fi

  if [[ "$INSTALL_PY_DEPS" != "1" ]]; then
    echo "Python runtime dependencies are missing; set PEEKABOOX_WINDOWS_LIVE_INSTALL_PY_DEPS=1 to create a local venv" >&2
    return 1
  fi

  python3 -m venv "$PY_RUNTIME_DIR"
  "$PY_RUNTIME_DIR/bin/python" -m pip install --upgrade pip
  "$PY_RUNTIME_DIR/bin/python" -m pip install -e "$ROOT/python[dev]"
  PYTHON_RUNTIME="$PY_RUNTIME_DIR/bin/python"
  echo "python runtime: $PYTHON_RUNTIME"
}

write_direct_all_json() {
  run_peekaboox windows \
    --backend "$WINDOWS_BACKEND" \
    --sort focused \
    --diagnose \
    --json >"$DIRECT_ALL_JSON"
}

write_direct_focused_json() {
  run_peekaboox windows \
    --backend "$WINDOWS_BACKEND" \
    --focused \
    --limit 1 \
    --sort focused \
    --json >"$DIRECT_FOCUSED_JSON"
}

write_query_json() {
  python3 - "$DIRECT_FOCUSED_JSON" "$DIRECT_ALL_JSON" "$QUERY_JSON" "$WINDOWS_BACKEND" <<'PY'
import json
import re
import sys

focused_path, all_path, query_path, backend = sys.argv[1:5]

def windows_from(path):
    payload = json.loads(open(path, encoding="utf-8").read())
    if isinstance(payload, list):
        return payload
    return payload.get("windows", [])

focused = windows_from(focused_path)
all_windows = windows_from(all_path)
candidates = focused + all_windows
target = None
for window in candidates:
    bounds = window.get("bounds") or {}
    if window.get("id") and window.get("title") and bounds.get("width", 0) > 0:
        target = window
        break
if target is None:
    raise SystemExit("no suitable window with id, title, and bounds found")

title = str(target["title"])
match = re.search(r"[A-Za-z0-9][A-Za-z0-9 .:_/-]{2,}", title)
title_fragment = match.group(0).strip() if match else title[:16]
if not title_fragment:
    raise SystemExit("selected window has no usable title fragment")
app = str(target.get("app_id") or title_fragment)
app_fragment = app.split()[0] if app.split() else app

query = {
    "id": target["id"],
    "app": app_fragment,
    "title": title_fragment,
    "title_regex": f".*{re.escape(title_fragment)}.*",
    "backend": backend,
    "target_title": title,
    "target_app_id": target.get("app_id") or "",
}
with open(query_path, "w", encoding="utf-8") as handle:
    json.dump(query, handle, indent=2, sort_keys=True)
print(json.dumps(query, indent=2, sort_keys=True))
PY
}

query_field() {
  python3 - "$QUERY_JSON" "$1" <<'PY'
import json
import sys

print(json.load(open(sys.argv[1], encoding="utf-8"))[sys.argv[2]])
PY
}

write_direct_app_json() {
  run_peekaboox windows \
    --backend "$WINDOWS_BACKEND" \
    --app "$(query_field app)" \
    --limit 5 \
    --sort focused \
    --json >"$DIRECT_APP_JSON"
}

write_direct_title_json() {
  run_peekaboox windows \
    --backend "$WINDOWS_BACKEND" \
    --title "$(query_field title)" \
    --limit 5 \
    --sort focused \
    --json >"$DIRECT_TITLE_JSON"
}

write_direct_regex_json() {
  run_peekaboox windows \
    --backend "$WINDOWS_BACKEND" \
    --title-regex "$(query_field title_regex)" \
    --limit 5 \
    --sort focused \
    --diagnose \
    --json >"$DIRECT_REGEX_JSON"
}

write_direct_id_json() {
  run_peekaboox windows \
    --backend "$WINDOWS_BACKEND" \
    --id "$(query_field id)" \
    --json >"$DIRECT_ID_JSON"
}

write_daemon_focused_json() {
  run_peekaboox --daemon \
    --socket "$SOCKET" \
    windows \
    --backend "$WINDOWS_BACKEND" \
    --focused \
    --limit 1 \
    --sort focused \
    --json >"$DAEMON_FOCUSED_JSON"
}

write_daemon_filtered_json() {
  run_peekaboox --daemon \
    --socket "$SOCKET" \
    windows \
    --backend "$WINDOWS_BACKEND" \
    --id "$(query_field id)" \
    --app "$(query_field app)" \
    --title "$(query_field title)" \
    --title-regex "$(query_field title_regex)" \
    --focused \
    --limit 5 \
    --sort focused \
    --diagnose \
    --json >"$DAEMON_FILTERED_JSON"
}

run_agent_windows() {
  PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
    "$PYTHON_RUNTIME" - "$GRPC_ADDR" "$@" <<'PY'
import sys
from peekaboox.agent.runtime import main

target = sys.argv[1]
args = ["--target", target, *sys.argv[2:]]
raise SystemExit(main(args))
PY
}

write_agent_focused_json() {
  run_agent_windows \
    windows \
    --focused \
    --limit 1 \
    --sort focused \
    --backend "$WINDOWS_BACKEND" >"$AGENT_FOCUSED_JSON"
}

write_agent_filtered_json() {
  run_agent_windows \
    windows \
    --id "$(query_field id)" \
    --app "$(query_field app)" \
    --title "$(query_field title)" \
    --title-regex "$(query_field title_regex)" \
    --focused \
    --limit 5 \
    --sort focused \
    --backend "$WINDOWS_BACKEND" \
    --diagnose >"$AGENT_FILTERED_JSON"
}

write_mcp_filtered_json() {
  PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
    "$PYTHON_RUNTIME" - "$GRPC_ADDR" "$QUERY_JSON" >"$MCP_FILTERED_JSON" <<'PY'
import json
import sys

from peekaboox.mcp.server import create_server

target, query_path = sys.argv[1:3]
query = json.load(open(query_path, encoding="utf-8"))
server = create_server(target, connect=True, capability_profile="observe")
result = server.handle_jsonrpc(
    {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "list_windows",
            "arguments": {
                "id": query["id"],
                "app": query["app"],
                "title": query["title"],
                "title_regex": query["title_regex"],
                "focused": True,
                "limit": 5,
                "sort": "focused",
                "backend": query["backend"],
                "diagnose": True,
            },
        },
    }
)
if result is None:
    raise SystemExit("MCP JSON-RPC call returned no response")
if result.get("result", {}).get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
print(json.dumps(result, indent=2))
PY
}

validate_windows_present() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

path, label = sys.argv[1:3]
payload = json.load(open(path, encoding="utf-8"))
windows = payload if isinstance(payload, list) else payload.get("windows", [])
if not windows:
    raise SystemExit(f"{label}: no windows returned")
print(f"{label}: {len(windows)} window(s)")
PY
}

validate_target_id_present() {
  python3 - "$1" "$QUERY_JSON" "$2" <<'PY'
import json
import sys

path, query_path, label = sys.argv[1:4]
payload = json.load(open(path, encoding="utf-8"))
windows = payload if isinstance(payload, list) else payload.get("windows", [])
target_id = json.load(open(query_path, encoding="utf-8"))["id"]
if not any(window.get("id") == target_id for window in windows):
    raise SystemExit(f"{label}: target id {target_id!r} was not returned")
print(f"{label}: target id matched")
PY
}

validate_exact_id_result() {
  python3 - "$1" "$QUERY_JSON" "$2" <<'PY'
import json
import sys

path, query_path, label = sys.argv[1:4]
payload = json.load(open(path, encoding="utf-8"))
windows = payload if isinstance(payload, list) else payload.get("windows", [])
target_id = json.load(open(query_path, encoding="utf-8"))["id"]
if len(windows) != 1:
    raise SystemExit(f"{label}: expected one window, got {len(windows)}")
if windows[0].get("id") != target_id:
    raise SystemExit(f"{label}: expected id {target_id!r}, got {windows[0].get('id')!r}")
print(f"{label}: exact id matched")
PY
}

validate_focused_limit() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

path, label = sys.argv[1:3]
payload = json.load(open(path, encoding="utf-8"))
windows = payload if isinstance(payload, list) else payload.get("windows", [])
if len(windows) > 1:
    raise SystemExit(f"{label}: expected at most one window, got {len(windows)}")
if windows and not windows[0].get("focused"):
    raise SystemExit(f"{label}: returned window is not focused")
print(f"{label}: {len(windows)} focused window(s)")
PY
}

validate_diagnostics() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

path, label = sys.argv[1:3]
payload = json.load(open(path, encoding="utf-8"))
if "structuredContent" in payload.get("result", {}):
    payload = payload["result"]["structuredContent"]
if not payload.get("backend_name"):
    raise SystemExit(f"{label}: missing backend_name")
if "backend_reports" not in payload:
    raise SystemExit(f"{label}: missing backend_reports")
print(f"{label}: diagnostics via {payload['backend_name']}")
PY
}

validate_mcp_target() {
  python3 - "$MCP_FILTERED_JSON" "$QUERY_JSON" <<'PY'
import json
import sys

payload = json.load(open(sys.argv[1], encoding="utf-8"))
query = json.load(open(sys.argv[2], encoding="utf-8"))
if payload.get("id") != 1:
    raise SystemExit("unexpected MCP JSON-RPC id")
if payload.get("result", {}).get("isError"):
    raise SystemExit("MCP tool returned an error")
content = payload["result"]["structuredContent"]
windows = content.get("windows", [])
if not any(window.get("id") == query["id"] for window in windows):
    raise SystemExit("MCP list_windows did not return the target window")
print(f"MCP list_windows: {len(windows)} window(s) via {content['backend_name']}")
PY
}

if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
  skip_or_fail "no DISPLAY or WAYLAND_DISPLAY is available for a live desktop smoke test"
fi

mkdir -p "$OUT_DIR"
for path in \
  "$DIRECT_ALL_JSON" \
  "$DIRECT_FOCUSED_JSON" \
  "$DIRECT_APP_JSON" \
  "$DIRECT_TITLE_JSON" \
  "$DIRECT_REGEX_JSON" \
  "$DIRECT_ID_JSON" \
  "$DAEMON_FOCUSED_JSON" \
  "$DAEMON_FILTERED_JSON" \
  "$AGENT_FOCUSED_JSON" \
  "$AGENT_FILTERED_JSON" \
  "$MCP_FILTERED_JSON" \
  "$QUERY_JSON" \
  "$DAEMON_LOG" \
  "$AUDIT_LOG"; do
  if [[ -e "$path" ]]; then
    echo "error: refusing to overwrite existing file: $path" >&2
    exit 1
  fi
done

echo "PeekabooX windows live smoke output: $OUT_DIR"
echo "Window backend: $WINDOWS_BACKEND"

run_step "direct window inventory with diagnostics" write_direct_all_json
run_step "validate direct inventory" validate_windows_present "$DIRECT_ALL_JSON" "direct inventory"
run_step "direct focused window query" write_direct_focused_json
run_step "validate direct focused limit" validate_focused_limit "$DIRECT_FOCUSED_JSON" "direct focused query"
run_step "derive reusable live window filters" write_query_json

run_step "direct app filter" write_direct_app_json
run_step "validate direct app filter" validate_target_id_present "$DIRECT_APP_JSON" "direct app filter"
run_step "direct title filter" write_direct_title_json
run_step "validate direct title filter" validate_target_id_present "$DIRECT_TITLE_JSON" "direct title filter"
run_step "direct title regex diagnostics" write_direct_regex_json
run_step "validate direct title regex" validate_target_id_present "$DIRECT_REGEX_JSON" "direct title regex"
run_step "validate direct diagnostics" validate_diagnostics "$DIRECT_REGEX_JSON" "direct regex diagnostics"
run_step "direct id filter" write_direct_id_json
run_step "validate direct id filter" validate_exact_id_result "$DIRECT_ID_JSON" "direct id filter"

run_step "start observe-only daemon" start_daemon
run_step "daemon focused query over Unix socket" write_daemon_focused_json
run_step "validate daemon focused query" validate_focused_limit "$DAEMON_FOCUSED_JSON" "daemon focused query"
run_step "daemon combined filter diagnostics over Unix socket" write_daemon_filtered_json
run_step "validate daemon combined filters" validate_target_id_present "$DAEMON_FILTERED_JSON" "daemon combined filters"
run_step "validate daemon diagnostics" validate_diagnostics "$DAEMON_FILTERED_JSON" "daemon diagnostics"

if ensure_python_runtime; then
  run_step "agent focused query over gRPC" write_agent_focused_json
  run_step "validate agent focused query" validate_focused_limit "$AGENT_FOCUSED_JSON" "agent focused query"
  run_step "agent combined filter diagnostics over gRPC" write_agent_filtered_json
  run_step "validate agent combined filters" validate_target_id_present "$AGENT_FILTERED_JSON" "agent combined filters"
  run_step "validate agent diagnostics" validate_diagnostics "$AGENT_FILTERED_JSON" "agent diagnostics"
  run_step "MCP JSON-RPC list_windows combined filters" write_mcp_filtered_json
  run_step "validate MCP list_windows target" validate_mcp_target
  run_step "validate MCP diagnostics" validate_diagnostics "$MCP_FILTERED_JSON" "MCP diagnostics"
else
  run_step "prepare Python runtime dependencies for agent and MCP checks" false
fi

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX windows live smoke example passed."
fi
