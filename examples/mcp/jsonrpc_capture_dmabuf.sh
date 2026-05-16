#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_RUNTIME="${PEEKABOOX_PYTHON_BIN:-python3}"
IMPORT_TARGET="${PEEKABOOX_MCP_DMABUF_IMPORT:-compute}"

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

json_string() {
  "$PYTHON_RUNTIME" -c 'import json, sys; print(json.dumps(sys.argv[1]))' "$1"
}

TOOLS_REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
printf '%s\n' "$TOOLS_REQUEST" | run_mcp | "$PYTHON_RUNTIME" -c '
import json
import sys

payload = json.load(sys.stdin)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
tools = {tool["name"]: tool for tool in payload["result"]["tools"]}
for name in ("probe_dmabuf", "capture_dmabuf"):
    tool = tools.get(name)
    if tool is None:
        raise SystemExit(f"{name} tool missing")
    schema = tool.get("inputSchema", {})
    properties = schema.get("properties", {})
    import_schema = properties.get("import_target")
    if import_schema is None:
        raise SystemExit(f"{name} schema missing import_target")
    if import_schema.get("enum") != ["compute", "egl", "egl_texture"]:
        raise SystemExit(f"{name} import_target enum mismatch")
print(json.dumps({"tools": ["capture_dmabuf", "probe_dmabuf"], "schema": "checked"}, sort_keys=True))
'

if [[ "${PEEKABOOX_MCP_DMABUF_LIVE:-0}" != "1" ]]; then
  echo "Schema check completed. Set PEEKABOOX_MCP_DMABUF_LIVE=1 to call a live daemon through MCP."
  exit 0
fi

IMPORT_TARGET_JSON="$(json_string "$IMPORT_TARGET")"
LIVE_REQUEST='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"capture_dmabuf","arguments":{"import_target":'"$IMPORT_TARGET_JSON"'}}}'
printf '%s\n' "$LIVE_REQUEST" | run_mcp | IMPORT_TARGET="$IMPORT_TARGET" "$PYTHON_RUNTIME" -c '
import json
import os
import sys

expected = os.environ["IMPORT_TARGET"].replace("-", "_")
payload = json.load(sys.stdin)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
result = payload.get("result", {})
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
content = result.get("structuredContent", {})
if content.get("import_target") != expected:
    raise SystemExit(json.dumps(content, indent=2))
required = {"backend_name", "stream_node_id", "width", "height", "pixel_format", "planes"}
missing = sorted(required - set(content))
if missing:
    raise SystemExit(f"DMA-BUF response missing keys: {missing}")
print(json.dumps({"import_target": expected, "backend_name": content.get("backend_name")}, sort_keys=True))
'

echo "PeekabooX MCP capture_dmabuf JSON-RPC example passed."
