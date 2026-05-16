#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_RUNTIME="${PEEKABOOX_PYTHON_BIN:-python3}"

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

REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

printf '%s\n' "$REQUEST" | run_mcp | "$PYTHON_RUNTIME" -c '
import json
import sys

payload = json.load(sys.stdin)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
tools = {tool["name"]: tool for tool in payload["result"]["tools"]}
tool = tools.get("hotkey")
if tool is None:
    raise SystemExit("hotkey tool missing")
schema = tool.get("inputSchema", {})
properties = schema.get("properties", {})
required = {
    "keys",
    "dry_run",
    "backend",
    "delay_ms",
    "key_delay_ms",
    "repeat",
    "interval_ms",
    "release_before",
    "release_after",
}
missing = sorted(required - set(properties))
if missing:
    raise SystemExit(f"hotkey schema missing: {missing}")
if properties["backend"].get("enum") != ["auto", "ydotool", "xdotool"]:
    raise SystemExit("unexpected hotkey backend enum")
if properties["repeat"].get("minimum") != 1:
    raise SystemExit("repeat must require positive values")
print(json.dumps({"tool": "hotkey", "checked": sorted(required)}, sort_keys=True))
'

if [[ "${PEEKABOOX_MCP_HOTKEY_LIVE:-0}" == "1" ]]; then
  LIVE_REQUEST='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"hotkey","arguments":{"keys":["control+s"],"dry_run":true,"backend":"auto","delay_ms":25,"key_delay_ms":30,"repeat":2,"interval_ms":40,"release_before":true,"release_after":true}}}'
  printf '%s\n' "$LIVE_REQUEST" | run_mcp | "$PYTHON_RUNTIME" -c '
import json
import sys

payload = json.load(sys.stdin)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
result = payload.get("result", {})
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
content = result.get("structuredContent", {})
if not content.get("ok"):
    raise SystemExit(json.dumps(content, indent=2))
print(json.dumps(content, sort_keys=True))
'
fi

echo "PeekabooX MCP hotkey JSON-RPC example passed."
