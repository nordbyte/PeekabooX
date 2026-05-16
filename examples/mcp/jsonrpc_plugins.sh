#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_RUNTIME="${PEEKABOOX_PYTHON_BIN:-python3}"
PLUGIN_ROOT="$ROOT/examples/plugins"
PLUGIN_ID="org.peekaboox.examples.system-info"
PLUGIN_TOOL="system_info.uname"

json_string() {
  "$PYTHON_RUNTIME" -c 'import json, sys; print(json.dumps(sys.argv[1]))' "$1"
}

run_mcp() {
  if [[ -n "${PEEKABOOX_MCP_BIN:-}" ]]; then
    "$PEEKABOOX_MCP_BIN" --plugin-path "$PLUGIN_ROOT" "$@"
  elif command -v peekaboox-mcp >/dev/null 2>&1; then
    peekaboox-mcp --plugin-path "$PLUGIN_ROOT" "$@"
  else
    PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
      "$PYTHON_RUNTIME" -m peekaboox.mcp.server --plugin-path "$PLUGIN_ROOT" "$@"
  fi
}

PLUGIN_ROOT_JSON="$(json_string "$PLUGIN_ROOT")"

TOOLS_REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
printf '%s\n' "$TOOLS_REQUEST" | run_mcp | "$PYTHON_RUNTIME" -c '
import json
import sys

payload = json.load(sys.stdin)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
tools = {tool["name"]: tool for tool in payload["result"]["tools"]}
for name in ("list_plugins", "call_plugin_tool"):
    if name not in tools:
        raise SystemExit(f"{name} tool missing")
list_schema = tools["list_plugins"].get("inputSchema", {}).get("properties", {})
call_schema = tools["call_plugin_tool"].get("inputSchema", {}).get("properties", {})
for field in ("paths",):
    if field not in list_schema:
        raise SystemExit(f"list_plugins schema missing {field}")
for field in ("plugin_id", "tool", "arguments", "paths", "timeout_seconds", "max_output_bytes"):
    if field not in call_schema:
        raise SystemExit(f"call_plugin_tool schema missing {field}")
print(json.dumps({"tools": ["call_plugin_tool", "list_plugins"], "schema": "checked"}, sort_keys=True))
'

LIST_REQUEST='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_plugins","arguments":{"paths":['"$PLUGIN_ROOT_JSON"']}}}'
printf '%s\n' "$LIST_REQUEST" | run_mcp | PLUGIN_ID="$PLUGIN_ID" PLUGIN_TOOL="$PLUGIN_TOOL" "$PYTHON_RUNTIME" -c '
import json
import os
import sys

plugin_id = os.environ["PLUGIN_ID"]
plugin_tool = os.environ["PLUGIN_TOOL"]
payload = json.load(sys.stdin)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
result = payload.get("result", {})
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
content = result.get("structuredContent", {})
if content.get("sdk_version") != "peekaboox.plugin.v1":
    raise SystemExit(json.dumps(content, indent=2))
if content.get("errors"):
    raise SystemExit(json.dumps(content["errors"], indent=2))
plugins = content.get("plugins", [])

def plugin_identifier(plugin):
    manifest = plugin.get("manifest") if isinstance(plugin.get("manifest"), dict) else {}
    return plugin.get("id") or manifest.get("id")

def plugin_tools(plugin):
    manifest = plugin.get("manifest") if isinstance(plugin.get("manifest"), dict) else {}
    return plugin.get("tools") or manifest.get("tools") or []

plugin = next((item for item in plugins if plugin_identifier(item) == plugin_id), None)
if plugin is None:
    raise SystemExit(f"missing plugin: {plugin_id}")
tools = {tool.get("name") for tool in plugin_tools(plugin)}
if plugin_tool not in tools:
    raise SystemExit(f"missing plugin tool: {plugin_tool}")
print(json.dumps({"plugin": plugin_id, "tool": plugin_tool}, sort_keys=True))
'

CALL_REQUEST='{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"call_plugin_tool","arguments":{"plugin_id":"'"$PLUGIN_ID"'","tool":"'"$PLUGIN_TOOL"'","arguments":{},"paths":['"$PLUGIN_ROOT_JSON"'],"timeout_seconds":5.0,"max_output_bytes":65536}}}'
printf '%s\n' "$CALL_REQUEST" | run_mcp | PLUGIN_ID="$PLUGIN_ID" PLUGIN_TOOL="$PLUGIN_TOOL" "$PYTHON_RUNTIME" -c '
import json
import os
import sys

plugin_id = os.environ["PLUGIN_ID"]
plugin_tool = os.environ["PLUGIN_TOOL"]
payload = json.load(sys.stdin)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
result = payload.get("result", {})
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
content = result.get("structuredContent", {})
if not content.get("ok"):
    raise SystemExit(json.dumps(content, indent=2))
if content.get("plugin_id") != plugin_id or content.get("tool") != plugin_tool:
    raise SystemExit(json.dumps(content, indent=2))
plugin_result = content.get("result", {})
required = {"system", "node", "release", "version", "machine", "processor"}
missing = sorted(required - set(plugin_result))
if missing:
    raise SystemExit(f"plugin result missing keys: {missing}")
print(json.dumps({"plugin": plugin_id, "result_keys": sorted(plugin_result)}, sort_keys=True))
'

echo "PeekabooX MCP Plugin SDK JSON-RPC example passed."
