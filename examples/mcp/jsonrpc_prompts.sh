#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_RUNTIME="${PEEKABOOX_PYTHON_BIN:-python3}"

"$PYTHON_RUNTIME" - "$ROOT" <<'PY'
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

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


def telegram_installed():
    peekaboox_bin = os.environ.get("PEEKABOOX_BIN") or shutil.which("peekaboox")
    if peekaboox_bin:
        completed = subprocess.run(
            [
                peekaboox_bin,
                "desktop",
                "profiles",
                "--app",
                "telegram",
                "--check",
                "--json",
            ],
            text=True,
            capture_output=True,
            cwd=root,
            env=env,
        )
        if completed.returncode == 0:
            payload = json.loads(completed.stdout)
            for profile in payload.get("profiles", []):
                availability = profile.get("availability", {})
                if availability.get("installed"):
                    return True
                if (
                    availability.get("command_available")
                    or availability.get("desktop_entry_available")
                ):
                    return True
            return False

    desktop_dirs = [
        Path.home() / ".local/share/applications",
        Path("/usr/local/share/applications"),
        Path("/usr/share/applications"),
    ]
    desktop_ids = {
        "telegram-desktop.desktop",
        "org.telegram.desktop.desktop",
        "telegram-desktop_telegram-desktop.desktop",
    }
    return (
        shutil.which("telegram-desktop") is not None
        or shutil.which("telegram") is not None
        or any(
            (directory / desktop_id).is_file()
            for directory in desktop_dirs
            for desktop_id in desktop_ids
        )
    )

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
if telegram_installed():
    completion = call(
        "completion/complete",
        {
            "argument": {"name": "target", "value": "search"},
            "context": {"app": "telegram"},
        },
        request_id=4,
    )["completion"]
else:
    completion = {"values": [], "skipped": True}
if not completion.get("skipped") and "search-input" not in completion["values"]:
    raise SystemExit("target completion did not include Telegram search-input")

print(
    json.dumps(
        {
            "prompts": len(prompts),
            "completion_values": completion["values"],
            "telegram_completion": "skipped" if completion.get("skipped") else "checked",
        },
        sort_keys=True,
    )
)
PY

echo "PeekabooX MCP prompts JSON-RPC example passed."
