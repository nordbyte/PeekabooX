#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_ID="${PEEKABOOX_DESKTOP_PROFILES_PARITY_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/desktop-profiles-daemon-parity}}"
SOCKET="${PEEKABOOX_DESKTOP_PROFILES_SOCKET:-/tmp/pbx-prof-$RUN_ID.sock}"
AUDIT_LOG="$OUT_DIR/audit-$RUN_ID.jsonl"
DAEMON_LOG="$OUT_DIR/daemon-$RUN_ID.log"
CLI_JSON="$OUT_DIR/cli-$RUN_ID.json"
PYTHON_JSON="$OUT_DIR/python-$RUN_ID.json"
MCP_JSON="$OUT_DIR/mcp-$RUN_ID.json"
PYTHON_RUNTIME=""
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
    "$ROOT/rust/desktop/src/lib.rs" \
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
  rm -f "$SOCKET"
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
  for _ in $(seq 1 100); do
    if [[ -S "$SOCKET" ]]; then
      return 0
    fi
    if ! kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
      cat "$DAEMON_LOG" >&2 || true
      return 1
    fi
    sleep 0.1
  done

  cat "$DAEMON_LOG" >&2 || true
  echo "timed out waiting for daemon socket: $SOCKET" >&2
  return 1
}

wait_for_grpc() {
  local grpc_addr="$1"
  python3 - "$grpc_addr" <<'PY'
import socket
import sys
import time

host, port_text = sys.argv[1].split(":", 1)
port = int(port_text)
for _ in range(100):
    try:
        with socket.create_connection((host, port), timeout=0.2):
            raise SystemExit(0)
    except OSError:
        time.sleep(0.1)
raise SystemExit("timed out waiting for gRPC")
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
      return 0
    fi
    echo "PEEKABOOX_PYTHON_BIN cannot import grpc/protobuf/peekaboox" >&2
    return 1
  fi

  if [[ -x "$ROOT/.venv/bin/python" ]] && python_has_runtime_deps "$ROOT/.venv/bin/python"; then
    PYTHON_RUNTIME="$ROOT/.venv/bin/python"
    return 0
  fi

  if python_has_runtime_deps python3; then
    PYTHON_RUNTIME="python3"
    return 0
  fi

  echo "no Python runtime with grpc, protobuf, and peekaboox imports found" >&2
  echo "try: PEEKABOOX_PYTHON_BIN=/path/to/python $0" >&2
  return 1
}

start_daemon() {
  local grpc_addr="$1"
  run_peekabooxd run \
    --profile observe \
    --socket "$SOCKET" \
    --grpc-addr "$grpc_addr" \
    --audit-log "$AUDIT_LOG" \
    --no-emergency-hotkey >"$DAEMON_LOG" 2>&1 &
  DAEMON_PID="$!"

  wait_for_socket
  wait_for_grpc "$grpc_addr"
}

write_cli_json() {
  run_peekaboox --daemon --socket "$SOCKET" desktop profiles \
    --app telegram \
    --target message-input \
    --command flatpak \
    --supports type-into \
    --availability \
    --json >"$CLI_JSON"
}

write_python_json() {
  local grpc_addr="$1"
  PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
    "$PYTHON_RUNTIME" - "$grpc_addr" >"$PYTHON_JSON" <<'PY'
import json
import sys
from dataclasses import asdict

from peekaboox.client import PeekabooXClient

client = PeekabooXClient(target=sys.argv[1])
try:
    result = client.desktop_profiles(
        "telegram",
        target="message-input",
        command="flatpak",
        supports="type-into",
        check=True,
    )
    print(json.dumps(asdict(result), sort_keys=True))
finally:
    client.close()
PY
}

write_mcp_json() {
  local grpc_addr="$1"
  local request
  request='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"desktop_profiles","arguments":{"app":"telegram","target":"message-input","command":"flatpak","supports":"type-into","check":true}}}'
  printf '%s\n' "$request" | \
    PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
    "$PYTHON_RUNTIME" -m peekaboox.mcp.server \
      --target "$grpc_addr" \
      --capability-profile observe >"$MCP_JSON"
}

validate_parity() {
  python3 - "$CLI_JSON" "$PYTHON_JSON" "$MCP_JSON" <<'PY'
import json
import sys

cli_path, python_path, mcp_path = sys.argv[1:4]
cli = json.load(open(cli_path, encoding="utf-8"))
python = json.load(open(python_path, encoding="utf-8"))
mcp_rpc = json.load(open(mcp_path, encoding="utf-8"))

if "error" in mcp_rpc:
    raise SystemExit(json.dumps(mcp_rpc, indent=2))
mcp_result = mcp_rpc["result"]
if mcp_result.get("isError"):
    raise SystemExit(json.dumps(mcp_result, indent=2))
mcp = mcp_result["structuredContent"]

for label, payload in (("cli", cli), ("python", python), ("mcp", mcp)):
    if payload.get("schema_version") != "desktop-profiles.v1":
        raise SystemExit(f"{label}: unexpected schema_version {payload.get('schema_version')!r}")
    if payload.get("count") != 1:
        raise SystemExit(f"{label}: expected count=1, got {payload.get('count')!r}")

    profile = payload["profiles"][0]
    if profile.get("id") != "telegram":
        raise SystemExit(f"{label}: expected telegram profile, got {profile.get('id')!r}")

    targets = profile.get("targets", [])
    message_input = next((target for target in targets if target.get("name") == "message-input"), None)
    if message_input is None:
        raise SystemExit(f"{label}: message-input target missing")
    if "type-into" not in message_input.get("supports", []):
        raise SystemExit(f"{label}: message-input missing type-into support")

    commands = profile.get("commands", [])
    flatpak = next((command for command in commands if command.get("program") == "flatpak"), None)
    if flatpak is None:
        raise SystemExit(f"{label}: flatpak command missing")
    if flatpak.get("display") != "flatpak run org.telegram.desktop":
        raise SystemExit(f"{label}: flatpak command args were not preserved")

    availability = profile.get("availability", {})
    if availability.get("checked") is not True:
        raise SystemExit(f"{label}: availability check flag missing")

print(json.dumps(
    {
        "cli_count": cli["count"],
        "python_count": python["count"],
        "mcp_count": mcp["count"],
        "profile": cli["profiles"][0]["id"],
        "target": "message-input",
    },
    sort_keys=True,
))
PY
}

if [[ -e "$OUT_DIR" && ! -d "$OUT_DIR" ]]; then
  echo "output path exists and is not a directory: $OUT_DIR" >&2
  exit 1
fi
mkdir -p "$OUT_DIR"
if [[ -e "$SOCKET" ]]; then
  echo "refusing to overwrite existing socket path: $SOCKET" >&2
  exit 1
fi

ensure_python_runtime
GRPC_ADDR="${PEEKABOOX_DESKTOP_PROFILES_GRPC_ADDR:-$(pick_free_grpc_addr)}"

echo "PeekabooX desktop profile daemon parity output: $OUT_DIR"
echo "daemon socket: $SOCKET"
echo "daemon gRPC: $GRPC_ADDR"
echo "python runtime: $PYTHON_RUNTIME"

start_daemon "$GRPC_ADDR"
write_cli_json
write_python_json "$GRPC_ADDR"
write_mcp_json "$GRPC_ADDR"
validate_parity

printf '\nWrote:\n'
printf '  %s\n' "$CLI_JSON"
printf '  %s\n' "$PYTHON_JSON"
printf '  %s\n' "$MCP_JSON"
printf '  %s\n' "$DAEMON_LOG"
