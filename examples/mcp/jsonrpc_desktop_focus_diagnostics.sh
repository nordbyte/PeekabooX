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

APP="${PEEKABOOX_MCP_DESKTOP_FOCUS_APP:-text-editor}"
GRPC_ADDR="${PEEKABOOX_MCP_DESKTOP_FOCUS_GRPC_ADDR:-127.0.0.1:47777}"
GRPC_TIMEOUT="${PEEKABOOX_MCP_DESKTOP_FOCUS_GRPC_TIMEOUT:-20}"
WAIT_MS="${PEEKABOOX_MCP_DESKTOP_FOCUS_WAIT_MS:-500}"
OVERVIEW_WAIT_MS="${PEEKABOOX_MCP_DESKTOP_FOCUS_OVERVIEW_WAIT_MS:-1000}"

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

TOOLS_REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

printf '%s\n' "$TOOLS_REQUEST" | run_mcp | "$PYTHON_RUNTIME" -c '
import json
import sys

payload = json.load(sys.stdin)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
tools = {tool["name"]: tool for tool in payload["result"]["tools"]}
tool = tools.get("desktop_focus")
if tool is None:
    raise SystemExit("desktop_focus tool missing")
input_properties = tool.get("inputSchema", {}).get("properties", {})
input_required = {
    "app",
    "use_gnome_overview",
    "launch_if_needed",
    "wait_after_focus_ms",
    "overview_wait_ms",
    "window_title",
    "window_id",
    "verify",
}
missing_input = sorted(input_required - set(input_properties))
if missing_input:
    raise SystemExit(f"desktop_focus input schema missing: {missing_input}")
output_schema = tool.get("outputSchema", {})
output_properties = output_schema.get("properties", {})
if "focus_diagnostics" not in output_properties:
    raise SystemExit("desktop_focus outputSchema missing focus_diagnostics")
if "focus_diagnostics" not in output_schema.get("required", []):
    raise SystemExit("desktop_focus outputSchema should require focus_diagnostics")
focus_schema = output_properties["focus_diagnostics"]
if focus_schema.get("type") != "array" or focus_schema.get("items", {}).get("type") != "string":
    raise SystemExit("desktop_focus focus_diagnostics output schema must be string[]")
print(json.dumps({"tool": "desktop_focus", "schema": "focus_diagnostics"}, sort_keys=True))
'

if [[ "${PEEKABOOX_MCP_DESKTOP_FOCUS_LIVE:-0}" != "1" ]]; then
  echo "Schema check completed. Set PEEKABOOX_MCP_DESKTOP_FOCUS_LIVE=1 to call a live daemon through MCP."
  echo "Live mode expects peekabooxd on ${GRPC_ADDR}; override app with PEEKABOOX_MCP_DESKTOP_FOCUS_APP."
  exit 0
fi

LIVE_REQUEST="$(
  "$PYTHON_RUNTIME" - "$APP" "$WAIT_MS" "$OVERVIEW_WAIT_MS" <<'PY'
import json
import sys

app, wait_ms, overview_wait_ms = sys.argv[1:4]
print(json.dumps({
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/call",
    "params": {
        "name": "desktop_focus",
        "arguments": {
            "app": app,
            "verify": True,
            "wait_after_focus_ms": int(wait_ms),
            "overview_wait_ms": int(overview_wait_ms),
        },
    },
}, separators=(",", ":")))
PY
)"

printf '%s\n' "$LIVE_REQUEST" \
  | run_mcp --target "$GRPC_ADDR" --capability-profile operator --grpc-timeout "$GRPC_TIMEOUT" \
  | "$PYTHON_RUNTIME" -c '
import json
import sys

payload = json.load(sys.stdin)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
result = payload.get("result", {})
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
structured = result.get("structuredContent", {})
diagnostics = structured.get("focus_diagnostics")
if not isinstance(diagnostics, list) or not diagnostics:
    raise SystemExit("desktop_focus returned empty focus_diagnostics")
if not all(isinstance(item, str) and item for item in diagnostics):
    raise SystemExit("focus_diagnostics must contain non-empty strings")
if not any(item.startswith("verify:") for item in diagnostics):
    raise SystemExit("verified desktop_focus should include a verify diagnostic")
text_blocks = [
    item for item in result.get("content", [])
    if item.get("type") == "text" and isinstance(item.get("text"), str)
]
if not text_blocks:
    raise SystemExit("missing MCP text content compatibility block")
text_payload = json.loads(text_blocks[0]["text"])
if text_payload.get("focus_diagnostics") != diagnostics:
    raise SystemExit("text content focus_diagnostics differs from structuredContent")
summary = {
    "app": structured.get("app"),
    "action": structured.get("action"),
    "backend_name": structured.get("backend_name"),
    "verified": structured.get("verified"),
    "diagnostic_count": len(diagnostics),
    "last_diagnostic": diagnostics[-1],
}
print(json.dumps(summary, sort_keys=True))
'

echo "PeekabooX MCP desktop_focus diagnostics JSON-RPC example passed."
