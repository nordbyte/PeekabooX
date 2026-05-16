#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_ID="${PEEKABOOX_MCP_PREFLIGHT_ERROR_RUN_ID:-$(date +%Y%m%d-%H%M%S)}"
OUT_ROOT="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/mcp-preflight-error-client}"
OUT_DIR="$OUT_ROOT/$RUN_ID"
PYTHON_RUNTIME="${PEEKABOOX_PYTHON_BIN:-python3}"

if [[ -e "$OUT_DIR" ]]; then
  echo "output directory already exists: $OUT_DIR" >&2
  exit 1
fi
mkdir -p "$OUT_DIR"

FAKE_PEEKABOOX="$OUT_DIR/fake-peekaboox"
RESPONSE_JSON="$OUT_DIR/preflight-error-response.json"
CLIENT_DECISION_JSON="$OUT_DIR/client-decision.json"

cat >"$FAKE_PEEKABOOX" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" != "doctor" || "${2:-}" != "--json" ]]; then
  echo "fake-peekaboox only supports: doctor --json" >&2
  exit 2
fi

cat <<'JSON'
{
  "status": "fail",
  "checks": [
    {
      "name": "input-click",
      "status": "fail",
      "detail": "no input backend candidate detected",
      "category": "input",
      "severity": "error"
    }
  ],
  "categories": [
    {
      "name": "input",
      "status": "fail",
      "severity": "error",
      "ok_count": 0,
      "warn_count": 0,
      "fail_count": 1,
      "total_count": 1
    }
  ]
}
JSON
SH
chmod +x "$FAKE_PEEKABOOX"

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

REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"click","arguments":{"x":10,"y":20}}}'

echo "PeekabooX MCP preflight-error client output: $OUT_DIR"
printf '%s\n' "$REQUEST" \
  | PEEKABOOX_BIN="$FAKE_PEEKABOOX" run_mcp --preflight-mode strict --preflight-timeout 5 \
  >"$RESPONSE_JSON"

"$PYTHON_RUNTIME" - "$RESPONSE_JSON" "$CLIENT_DECISION_JSON" <<'PY'
import json
import sys

response_path = sys.argv[1]
decision_path = sys.argv[2]
payload = json.load(open(response_path, encoding="utf-8"))

if payload.get("id") != 1:
    raise SystemExit(f"unexpected JSON-RPC id: {payload.get('id')}")
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))

result = payload.get("result", {})
if result.get("isError") is not True:
    raise SystemExit("expected a structured MCP tool error")

data = result.get("structuredContent", {})
if data.get("error") != "PreflightError":
    raise SystemExit(f"unexpected error type: {data.get('error')}")
if data.get("tool") != "click":
    raise SystemExit(f"unexpected tool: {data.get('tool')}")
if data.get("next_action") != "run_doctor":
    raise SystemExit(f"unexpected next_action: {data.get('next_action')}")
if data.get("blocked_categories") != ["input"]:
    raise SystemExit(f"unexpected blocked_categories: {data.get('blocked_categories')}")
if data.get("required_categories") != ["input"]:
    raise SystemExit(f"unexpected required_categories: {data.get('required_categories')}")

preflight = data.get("preflight", {})
if preflight.get("operation") != "click":
    raise SystemExit(f"unexpected preflight operation: {preflight.get('operation')}")
if preflight.get("blocked_categories") != ["input"]:
    raise SystemExit(f"unexpected preflight blocked_categories: {preflight.get('blocked_categories')}")
if preflight.get("category_status", {}).get("input") != "fail":
    raise SystemExit("preflight category_status.input should be fail")
if preflight.get("category_severity", {}).get("input") != "error":
    raise SystemExit("preflight category_severity.input should be error")

text_blocks = result.get("content", [])
if not text_blocks or text_blocks[0].get("type") != "text":
    raise SystemExit("missing MCP text content compatibility block")
text_payload = json.loads(text_blocks[0].get("text", "{}"))
if text_payload.get("blocked_categories") != ["input"]:
    raise SystemExit("text content block does not mirror blocked_categories")

decision = {
    "client_decision": "show_doctor_action",
    "next_action": data["next_action"],
    "blocked_categories": data["blocked_categories"],
    "message": data["message"],
}
with open(decision_path, "w", encoding="utf-8") as handle:
    json.dump(decision, handle, indent=2, sort_keys=True)
    handle.write("\n")
print(json.dumps(decision, sort_keys=True))
PY

echo "PeekabooX MCP preflight-error client example passed."
