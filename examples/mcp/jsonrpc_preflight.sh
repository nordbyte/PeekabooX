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

REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"preflight","arguments":{"categories":["desktop","capture"],"operation":"capture_screen","require":false,"timeout_seconds":30}}}'

printf '%s\n' "$REQUEST" | run_mcp | "$PYTHON_RUNTIME" -c '
import json
import sys

payload = json.load(sys.stdin)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
result = payload.get("result", {})
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
data = result.get("structuredContent", {})
if not isinstance(data.get("ok"), bool):
    raise SystemExit("preflight result missing boolean ok")
required = data.get("required_categories", [])
if required != ["desktop", "capture"]:
    raise SystemExit(f"unexpected required categories: {required}")
for key in ("blocked_categories", "warning_categories", "messages"):
    if not isinstance(data.get(key), list):
        raise SystemExit(f"preflight result missing list {key}")
category_status = data.get("category_status", {})
if not isinstance(category_status, dict):
    raise SystemExit("preflight result missing category_status")
print(
    json.dumps(
        {
            "ok": data["ok"],
            "blocked": data["blocked_categories"],
            "warnings": data["warning_categories"],
            "status": {
                category: category_status.get(category)
                for category in required
            },
        },
        sort_keys=True,
    )
)
'

echo "PeekabooX MCP preflight JSON-RPC example passed."
