#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_ID="${PEEKABOOX_MCP_DOCTOR_RUN_ID:-$(date +%Y%m%d-%H%M%S)}"
OUT_ROOT="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/mcp-doctor}"
OUT_DIR="$OUT_ROOT/$RUN_ID"
PYTHON_RUNTIME="${PEEKABOOX_PYTHON_BIN:-python3}"

if [[ -e "$OUT_DIR" ]]; then
  echo "output directory already exists: $OUT_DIR" >&2
  exit 1
fi
mkdir -p "$OUT_DIR"

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

REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"doctor","arguments":{"strict":false,"timeout_seconds":30}}}'
RESPONSE_JSON="$OUT_DIR/doctor-response.json"

echo "PeekabooX MCP doctor JSON-RPC output: $OUT_DIR"
printf '%s\n' "$REQUEST" | run_mcp >"$RESPONSE_JSON"

"$PYTHON_RUNTIME" - "$RESPONSE_JSON" <<'PY'
import json
import sys

path = sys.argv[1]
payload = json.load(open(path, encoding="utf-8"))
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
result = payload.get("result", {})
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
data = result.get("structuredContent", {})
checks = data.get("checks", [])
if data.get("status") not in {"ok", "fail"}:
    raise SystemExit(f"unexpected doctor status: {data.get('status')}")
if not checks:
    raise SystemExit("doctor returned no checks")
names = {check.get("name") for check in checks}
required = {"desktop-session", "display-server", "desktop-profiles"}
missing = sorted(required - names)
if missing:
    raise SystemExit(f"missing doctor checks: {', '.join(missing)}")
for key in ("ok_count", "warn_count", "fail_count"):
    if not isinstance(data.get(key), int):
        raise SystemExit(f"doctor result missing integer {key}")
print(
    json.dumps(
        {
            "status": data["status"],
            "checks": len(checks),
            "ok": data["ok_count"],
            "warn": data["warn_count"],
            "fail": data["fail_count"],
        },
        sort_keys=True,
    )
)
PY

echo "PeekabooX MCP doctor JSON-RPC example passed."
