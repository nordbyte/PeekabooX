#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_RUNTIME="${PEEKABOOX_PYTHON_BIN:-python3}"

REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

"$PYTHON_RUNTIME" - "$ROOT" "$REQUEST" <<'PY'
import json
import os
import subprocess
import sys

root = sys.argv[1]
request = sys.argv[2]
env = os.environ.copy()
env["PYTHONPATH"] = f"{root}/python/src" + (f":{env['PYTHONPATH']}" if env.get("PYTHONPATH") else "")
completed = subprocess.run(
    [sys.executable, "-m", "peekaboox.mcp.server"],
    input=request + "\n",
    text=True,
    capture_output=True,
    check=True,
    env=env,
)
payload = json.loads(completed.stdout)
if "error" in payload:
    raise SystemExit(json.dumps(payload, indent=2))
tools = payload["result"]["tools"]
by_name = {tool["name"]: tool for tool in tools}
required = {
    "capture_screen",
    "capture_delta",
    "capture_backends",
    "capture_dmabuf",
    "find_element",
    "find_elements",
    "elements",
    "ocr",
    "ocr_image",
    "vision_elements",
    "desktop_profiles",
    "plan",
    "plan_workflow",
    "replan_workflow",
    "query_desktop_edges",
    "capability_audit",
    "confirmation_audit",
    "preflight_audit",
}
missing = sorted(required - set(by_name))
if missing:
    raise SystemExit(f"missing MCP tools: {', '.join(missing)}")
for name in required:
    tool = by_name[name]
    if "inputSchema" not in tool:
        raise SystemExit(f"{name} missing inputSchema")
    if "outputSchema" not in tool:
        raise SystemExit(f"{name} missing outputSchema")
    if "annotations" not in tool:
        raise SystemExit(f"{name} missing annotations")
if not by_name["capture_screen"]["annotations"]["readOnlyHint"]:
    raise SystemExit("capture_screen should be annotated read-only")
if not by_name["click"]["annotations"]["destructiveHint"]:
    raise SystemExit("click should be annotated destructive")
print(json.dumps({"tools": len(tools), "checked": len(required)}, sort_keys=True))
PY

echo "PeekabooX MCP tool parity JSON-RPC example passed."
