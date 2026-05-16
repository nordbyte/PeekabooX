#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_RUNTIME="${PEEKABOOX_PYTHON_BIN:-python3}"

"$PYTHON_RUNTIME" - "$ROOT" <<'PY'
import json
import os
import subprocess
import sys

root = sys.argv[1]
env = os.environ.copy()
env["PYTHONPATH"] = f"{root}/python/src" + (f":{env['PYTHONPATH']}" if env.get("PYTHONPATH") else "")

def call(method, params=None, request_id=1):
    request = {"jsonrpc": "2.0", "id": request_id, "method": method}
    if params is not None:
        request["params"] = params
    completed = subprocess.run(
        [
            sys.executable,
            "-m",
            "peekaboox.mcp.server",
            "--plugin-path",
            f"{root}/examples/plugins",
        ],
        input=json.dumps(request) + "\n",
        text=True,
        capture_output=True,
        check=True,
        env=env,
    )
    payload = json.loads(completed.stdout)
    if "error" in payload:
        raise SystemExit(json.dumps(payload, indent=2))
    return payload["result"]

resources = call("resources/list", request_id=1)["resources"]
uris = {resource["uri"] for resource in resources}
required = {
    "peekaboox://server/info",
    "peekaboox://tools",
    "peekaboox://desktop/profiles",
    "peekaboox://docs/runtime",
}
missing = sorted(required - uris)
if missing:
    raise SystemExit(f"missing resources: {', '.join(missing)}")

info = call("resources/read", {"uri": "peekaboox://server/info"}, request_id=2)
server_info = json.loads(info["contents"][0]["text"])
if server_info["name"] != "peekaboox-mcp":
    raise SystemExit("unexpected server info resource")
if not server_info["capabilities"]["resources"]:
    raise SystemExit("server info does not report resources capability")

docs = call("resources/read", {"uri": "peekaboox://docs/runtime"}, request_id=3)
if "Python Runtime" not in docs["contents"][0]["text"]:
    raise SystemExit("runtime docs resource missing expected heading")

templates = call("resources/templates/list", request_id=4)["resourceTemplates"]
if "docs" not in {template["name"] for template in templates}:
    raise SystemExit("docs resource template missing")

print(
    json.dumps(
        {
            "resources": len(resources),
            "server": server_info["name"],
            "templates": len(templates),
        },
        sort_keys=True,
    )
)
PY

echo "PeekabooX MCP resources JSON-RPC example passed."
