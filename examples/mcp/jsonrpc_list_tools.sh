#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

run_mcp() {
  if [[ -n "${PEEKABOOX_MCP_BIN:-}" ]]; then
    "$PEEKABOOX_MCP_BIN" --plugin-path "$ROOT/examples/plugins" "$@"
  elif command -v peekaboox-mcp >/dev/null 2>&1; then
    peekaboox-mcp --plugin-path "$ROOT/examples/plugins" "$@"
  else
    PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
      python3 -m peekaboox.mcp.server --plugin-path "$ROOT/examples/plugins" "$@"
  fi
}

echo "== MCP tool registry summary =="
tools_line="$(run_mcp --list-tools)"
echo "$tools_line"
grep -q "capture_screen" <<<"$tools_line"
grep -q "list_plugins" <<<"$tools_line"
grep -q "execute_workflow" <<<"$tools_line"
grep -q "preflight" <<<"$tools_line"

echo "== MCP JSON-RPC tools/list =="
response="$(
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | run_mcp
)"
echo "$response"

python3 - "$response" <<'PY'
from __future__ import annotations

import json
import sys

payload = json.loads(sys.argv[1])
tools = payload["result"]["tools"]
names = {tool["name"] for tool in tools}
required = {"capture_screen", "list_plugins", "execute_workflow", "preflight"}
missing = sorted(required - names)
if missing:
    raise SystemExit(f"missing MCP tools: {', '.join(missing)}")
print(json.dumps({"tool_count": len(tools), "required": sorted(required)}, sort_keys=True))
PY

echo "PeekabooX MCP JSON-RPC example passed."
