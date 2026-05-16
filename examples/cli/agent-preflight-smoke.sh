#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_RUNTIME="${PEEKABOOX_PYTHON_BIN:-python3}"

run_agent() {
  if [[ -n "${PEEKABOOX_AGENT_BIN:-}" ]]; then
    "$PEEKABOOX_AGENT_BIN" "$@"
  elif command -v peekaboox-agent >/dev/null 2>&1; then
    peekaboox-agent "$@"
  else
    PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" \
      "$PYTHON_RUNTIME" -c 'from peekaboox.agent.runtime import main; raise SystemExit(main())' "$@"
  fi
}

args=(
  --preflight-mode strict
  --preflight-timeout "${PEEKABOOX_PREFLIGHT_TIMEOUT:-30}"
  preflight
  desktop
  capture
  --operation capture_screen
  --timeout "${PEEKABOOX_PREFLIGHT_TIMEOUT:-30}"
)
if [[ "${PEEKABOOX_STRICT:-0}" == "1" ]]; then
  args+=(--require)
fi

response="$(run_agent "${args[@]}")"
printf '%s\n' "$response"

"$PYTHON_RUNTIME" - "$response" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
if not isinstance(payload.get("ok"), bool):
    raise SystemExit("preflight result missing boolean ok")
if payload.get("operation") != "capture_screen":
    raise SystemExit(f"unexpected operation: {payload.get('operation')}")
required = payload.get("required_categories")
if required != ["desktop", "capture"]:
    raise SystemExit(f"unexpected required categories: {required}")
for key in ("blocked_categories", "warning_categories", "messages"):
    if not isinstance(payload.get(key), list):
        raise SystemExit(f"preflight result missing list {key}")
category_status = payload.get("category_status")
if not isinstance(category_status, dict):
    raise SystemExit("preflight result missing category_status")
for category in required:
    if category not in category_status:
        raise SystemExit(f"preflight result missing category status for {category}")
print(
    json.dumps(
        {
            "ok": payload["ok"],
            "blocked": payload["blocked_categories"],
            "warnings": payload["warning_categories"],
            "status": {
                category: category_status.get(category)
                for category in required
            },
        },
        sort_keys=True,
    )
)
PY

echo "PeekabooX agent preflight smoke example passed."
