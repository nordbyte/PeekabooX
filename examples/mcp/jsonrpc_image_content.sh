#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON_RUNTIME="${PEEKABOOX_PYTHON_BIN:-python3}"

PYTHONPATH="$ROOT/python/src${PYTHONPATH:+:$PYTHONPATH}" "$PYTHON_RUNTIME" - <<'PY'
import json

from peekaboox.agent import AgentRuntime
from peekaboox.client import CaptureMetadata, CaptureScreenResult
from peekaboox.mcp import McpServer


class FakeClient:
    def capture_screen(self, include_semantic_tree=False, region=None, window_id=None):
        return CaptureScreenResult(
            image=b"png",
            mime_type="image/png",
            semantic_tree=(),
            metadata=CaptureMetadata(
                width=2,
                height=2,
                backend="fake",
                captured_at_unix_ms=123,
            ),
        )


server = McpServer(runtime=AgentRuntime(client=FakeClient()))
server.register_default_tools()
response = server.handle_jsonrpc(
    {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "capture_screen", "arguments": {}},
    }
)
if "error" in response:
    raise SystemExit(json.dumps(response, indent=2))
result = response["result"]
if result.get("isError"):
    raise SystemExit(json.dumps(result, indent=2))
content = result["content"]
if content[0]["type"] != "image" or content[0]["mimeType"] != "image/png":
    raise SystemExit("capture_screen did not return MCP image content")
text_payload = json.loads(content[1]["text"])
if text_payload["image_base64"] != "cG5n":
    raise SystemExit("text compatibility block did not include base64 image")
print(json.dumps({"content_types": [item["type"] for item in content]}, sort_keys=True))
PY

echo "PeekabooX MCP image content JSON-RPC example passed."
