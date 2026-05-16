#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_RUNTIME="${PEEKABOOX_PYTHON_BIN:-python3}"

PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" "$PYTHON_RUNTIME" - <<'PY'
import json
from unittest.mock import patch

from peekaboox.agent import AgentRuntime
from peekaboox.client import ActionResult
from peekaboox.doctor import DoctorCategory, DoctorCheck, DoctorResult
from peekaboox.mcp import McpServer
from peekaboox.security import (
    Capability,
    CapabilityPolicy,
    ConfirmationPolicy,
    DangerousAction,
)


class FakeClient:
    def click(self, x=None, y=None, semantic_selector=None, vision_fallback=False):
        return ActionResult(ok=True, message="clicked")

    def type_text(self, text, typing_speed_chars_per_second=None):
        return ActionResult(ok=True, message="typed")


def tool_call(server, name, arguments, request_id):
    return server.handle_jsonrpc(
        {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments},
        }
    )["result"]["structuredContent"]


doctor = DoctorResult(
    status="fail",
    checks=(
        DoctorCheck(
            name="input-click",
            status="fail",
            detail="no input backend candidate detected",
        ),
    ),
    categories=(
        DoctorCategory(
            name="input",
            status="fail",
            severity="error",
            ok_count=0,
            warn_count=0,
            fail_count=1,
            total_count=1,
        ),
    ),
    ok_count=0,
    warn_count=0,
    fail_count=1,
    exit_code=0,
)

preflight_server = McpServer(runtime=AgentRuntime(client=FakeClient(), preflight_mode="strict"))
preflight_server.register_default_tools()
with patch("peekaboox.agent.runtime.run_doctor", return_value=doctor):
    preflight_error = tool_call(preflight_server, "click", {"x": 10, "y": 20}, 1)

capability_server = McpServer(
    runtime=AgentRuntime(
        client=FakeClient(),
        capability_policy=CapabilityPolicy.deny([Capability.CLICK]),
    )
)
capability_server.register_default_tools()
capability_error = tool_call(capability_server, "click", {"x": 10, "y": 20}, 2)

confirmation_server = McpServer(
    runtime=AgentRuntime(
        client=FakeClient(),
        confirmation_policy=ConfirmationPolicy.require_for([DangerousAction.TYPE_TEXT]),
    )
)
confirmation_server.register_default_tools()
confirmation_error = tool_call(confirmation_server, "type_text", {"text": "Hello"}, 3)

matrix = {
    "preflight": preflight_error["next_action"],
    "capability": capability_error["next_action"],
    "confirmation": confirmation_error["next_action"],
}
expected = {
    "preflight": "run_doctor",
    "capability": "adjust_capability_profile",
    "confirmation": "request_confirmation",
}
if matrix != expected:
    raise SystemExit(json.dumps(matrix, indent=2))
print(json.dumps(matrix, sort_keys=True))
PY

echo "PeekabooX MCP recovery matrix JSON-RPC example passed."
