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
tool = tools.get("paste_text")
if tool is None:
    raise SystemExit("paste_text tool missing")
schema = tool.get("inputSchema", {})
properties = schema.get("properties", {})
required = {
    "text",
    "preserve_clipboard",
    "dry_run",
    "clipboard_backend",
    "hotkey_backend",
    "delay_ms",
    "restore_delay_ms",
    "restore_policy",
}
missing = sorted(required - set(properties))
if missing:
    raise SystemExit(f"paste_text schema missing: {missing}")
if properties["clipboard_backend"].get("enum") != ["auto", "wl-copy", "xclip", "xsel"]:
    raise SystemExit("unexpected clipboard backend enum")
if properties["hotkey_backend"].get("enum") != ["auto", "ydotool", "xdotool"]:
    raise SystemExit("unexpected hotkey backend enum")
if properties["restore_policy"].get("enum") != ["strict", "best-effort", "off"]:
    raise SystemExit("unexpected restore policy enum")
print(json.dumps({"tool": "paste_text", "checked": sorted(required)}, sort_keys=True))
'

if [[ "${PEEKABOOX_MCP_PASTE_LIVE:-0}" == "1" ]]; then
  LIVE_REQUEST='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"paste_text","arguments":{"text":"PeekabooX MCP paste example","preserve_clipboard":true,"dry_run":true,"clipboard_backend":"auto","hotkey_backend":"auto","delay_ms":80,"restore_delay_ms":120,"restore_policy":"best-effort"}}}'
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

echo "PeekabooX MCP paste_text JSON-RPC example passed."
