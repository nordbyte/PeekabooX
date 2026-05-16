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
        [sys.executable, "-m", "peekaboox.mcp.server"],
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

prompts = call("prompts/list", request_id=1)["prompts"]
names = {prompt["name"] for prompt in prompts}
for expected in ("build-workflow", "recover-from-tool-error", "safe-desktop-action"):
    if expected not in names:
        raise SystemExit(f"missing prompt: {expected}")

prompt = call(
    "prompts/get",
    {
        "name": "build-workflow",
        "arguments": {"goal": "Open Telegram Saved Messages", "format": "yaml"},
    },
    request_id=2,
)
text = prompt["messages"][0]["content"]["text"]
if "Open Telegram Saved Messages" not in text or "editable workflow" not in text:
    raise SystemExit("prompt text did not include expected guidance")

call("logging/setLevel", {"level": "warning"}, request_id=3)
completion = call(
    "completion/complete",
    {
        "argument": {"name": "target", "value": "search"},
        "context": {"app": "telegram"},
    },
    request_id=4,
)["completion"]
if "search-input" not in completion["values"]:
    raise SystemExit("target completion did not include Telegram search-input")

print(
    json.dumps(
        {
            "prompts": len(prompts),
            "completion_values": completion["values"],
        },
        sort_keys=True,
    )
)
PY

echo "PeekabooX MCP prompts JSON-RPC example passed."
