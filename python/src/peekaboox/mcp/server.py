from __future__ import annotations

import argparse
import base64
import http.server
import json
import os
import sys
from collections.abc import Callable
from dataclasses import dataclass, field, fields, is_dataclass
from pathlib import Path
from typing import Any, TextIO

from peekaboox.agent import AgentRuntime, PreflightError, WorkflowExecutionResult
from peekaboox.agent.runtime import (
    HOTKEY_BACKEND_CHOICES,
    TYPE_BACKEND_CHOICES,
    WINDOW_BACKEND_CHOICES,
    WINDOW_SORT_CHOICES,
)
from peekaboox.client import DEFAULT_GRPC_TIMEOUT_SECONDS, Rect
from peekaboox.security import (
    KNOWN_CAPABILITY_PROFILES,
    CapabilityPolicy,
    CapabilityDeniedError,
    ConfirmationDeniedError,
    ConfirmationPolicy,
    ConfirmationRequiredError,
    JsonlAuditLogger,
)
from peekaboox.workflows import dump_workflow_text, workflow_from_dict, workflow_to_dict


MCP_PROTOCOL_VERSION = "2025-11-25"
SERVER_NAME = "peekaboox-mcp"
SERVER_VERSION = "1.1.0"

PARSE_ERROR = -32700
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INVALID_PARAMS = -32602
INTERNAL_ERROR = -32603

LOG_LEVELS = (
    "debug",
    "info",
    "notice",
    "warning",
    "error",
    "critical",
    "alert",
    "emergency",
)

PREFLIGHT_CATEGORIES = ("desktop", "capture", "input", "ocr", "python")
MOVE_BACKEND_CHOICES = ("auto", "uinput", "ydotool", "xdotool")
DRAG_BACKEND_CHOICES = ("auto", "uinput", "xdotool")
MOVE_BOUNDS_POLICY_CHOICES = ("allow", "clamp", "fail", "fail-out-of-bounds")
WORKFLOW_ACTIONS = (
    "observe",
    "capture_screen",
    "find_element",
    "click",
    "move_mouse",
    "drag",
    "type_text",
    "paste_text",
    "hotkey",
    "list_windows",
    "get_desktop_state",
)

DOC_RESOURCES = {
    "api": "docs/api.md",
    "runtime": "docs/runtime.md",
    "security": "docs/security.md",
    "plugins": "docs/plugins.md",
    "cli": "docs/cli.md",
    "examples": "examples/README.md",
}

RECT_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {
        "x": {"type": "integer"},
        "y": {"type": "integer"},
        "width": {"type": "integer", "minimum": 0},
        "height": {"type": "integer", "minimum": 0},
    },
    "required": ["x", "y", "width", "height"],
    "additionalProperties": False,
}

WORKFLOW_STEP_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {
        "action": {"type": "string"},
        "selector": {"type": "string"},
        "value": {"type": "string"},
        "x": {"type": "integer"},
        "y": {"type": "integer"},
        "from_x": {"type": "integer"},
        "from_y": {"type": "integer"},
        "to_x": {"type": "integer"},
        "to_y": {"type": "integer"},
        "from_current": {"type": "boolean", "default": False},
        "from_ratio_x": {"type": "number", "minimum": 0.0, "maximum": 1.0},
        "from_ratio_y": {"type": "number", "minimum": 0.0, "maximum": 1.0},
        "to_ratio_x": {"type": "number", "minimum": 0.0, "maximum": 1.0},
        "to_ratio_y": {"type": "number", "minimum": 0.0, "maximum": 1.0},
        "button": {"type": "string", "enum": ["left", "middle", "right"]},
        "duration_ms": {"type": "integer", "minimum": 0},
        "relative_x": {"type": "integer"},
        "relative_y": {"type": "integer"},
        "region": {"type": "string"},
        "ratio_x": {"type": "number", "minimum": 0.0, "maximum": 1.0},
        "ratio_y": {"type": "number", "minimum": 0.0, "maximum": 1.0},
        "window_id": {"type": "string"},
        "app": {"type": "string"},
        "window_title": {"type": "string"},
        "title_regex": {"type": "string"},
        "steps": {"type": "integer", "minimum": 1},
        "bounds_policy": {"type": "string", "enum": list(MOVE_BOUNDS_POLICY_CHOICES)},
        "backend": {
            "type": "string",
            "enum": sorted({*MOVE_BACKEND_CHOICES, *DRAG_BACKEND_CHOICES, *TYPE_BACKEND_CHOICES}),
        },
        "clipboard_backend": {
            "type": "string",
            "enum": ["auto", "wl-copy", "xclip", "xsel"],
        },
        "hotkey_backend": {"type": "string", "enum": list(HOTKEY_BACKEND_CHOICES)},
        "typing_speed_chars_per_second": {"type": "integer", "minimum": 1},
        "delay_ms": {"type": "integer", "minimum": 0},
        "key_delay_ms": {"type": "integer", "minimum": 0},
        "repeat": {"type": "integer", "minimum": 1},
        "interval_ms": {"type": "integer", "minimum": 0},
        "release_before": {"type": "boolean", "default": False},
        "release_after": {"type": "boolean", "default": False},
        "preserve_clipboard": {"type": "boolean", "default": False},
        "restore_delay_ms": {"type": "integer", "minimum": 0},
        "restore_policy": {
            "type": "string",
            "enum": ["strict", "best-effort", "off"],
        },
        "restore": {"type": "boolean", "default": False},
        "dry_run": {"type": "boolean", "default": False},
        "vision_fallback": {"type": "boolean", "default": False},
        "verify": {"type": "boolean", "default": True},
    },
    "required": ["action"],
    "additionalProperties": False,
}


class JsonRpcProtocolError(ValueError):
    def __init__(self, code: int, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


@dataclass(slots=True)
class McpTool:
    """Local MCP-compatible tool definition and callable handler."""

    name: str
    description: str
    input_schema: dict[str, Any]
    handler: Callable[[dict[str, Any]], Any] | None = None
    title: str | None = None
    output_schema: dict[str, Any] | None = None
    annotations: dict[str, Any] | None = None

    def descriptor(self) -> dict[str, Any]:
        descriptor = {
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        }
        if self.title:
            descriptor["title"] = self.title
        if self.output_schema is not None:
            descriptor["outputSchema"] = self.output_schema
        if self.annotations is not None:
            descriptor["annotations"] = self.annotations
        return descriptor

    def call(self, arguments: dict[str, Any] | None = None) -> Any:
        if self.handler is None:
            raise RuntimeError(f"MCP tool {self.name!r} is not bound to an AgentRuntime")
        return self.handler(arguments or {})

    def __call__(self, **arguments: Any) -> Any:
        return self.call(arguments)


@dataclass(frozen=True, slots=True)
class McpResource:
    uri: str
    name: str
    title: str
    description: str
    mime_type: str = "application/json"

    def descriptor(self) -> dict[str, Any]:
        return {
            "uri": self.uri,
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "mimeType": self.mime_type,
        }


@dataclass(frozen=True, slots=True)
class McpResourceTemplate:
    uri_template: str
    name: str
    title: str
    description: str
    mime_type: str = "text/markdown"

    def descriptor(self) -> dict[str, Any]:
        return {
            "uriTemplate": self.uri_template,
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "mimeType": self.mime_type,
        }


@dataclass(frozen=True, slots=True)
class McpPromptArgument:
    name: str
    description: str
    required: bool = False

    def descriptor(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "description": self.description,
            "required": self.required,
        }


@dataclass(frozen=True, slots=True)
class McpPrompt:
    name: str
    description: str
    arguments: tuple[McpPromptArgument, ...] = ()

    def descriptor(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "description": self.description,
            "arguments": [argument.descriptor() for argument in self.arguments],
        }


@dataclass(slots=True)
class McpServer:
    """Registry and dispatcher for PeekabooX MCP tool handlers."""

    runtime: AgentRuntime | None = None
    tools: dict[str, McpTool] = field(default_factory=dict)
    log_level: str = "info"

    def register_default_tools(self) -> None:
        for tool in self._default_tools():
            existing = self.tools.get(tool.name)
            if existing is None or (existing.handler is None and tool.handler is not None):
                self.tools[tool.name] = tool

    def register_tool(self, tool: McpTool) -> None:
        if not tool.name:
            raise ValueError("tool name must not be empty")
        self.tools[tool.name] = tool

    def list_tools(self) -> list[dict[str, Any]]:
        return [self.tools[name].descriptor() for name in sorted(self.tools)]

    def call_tool(self, name: str, arguments: dict[str, Any] | None = None) -> Any:
        try:
            tool = self.tools[name]
        except KeyError as error:
            raise ValueError(f"unknown MCP tool: {name}") from error
        return tool.call(arguments)

    def list_resources(self) -> list[dict[str, Any]]:
        return [resource.descriptor() for resource in self._default_resources()]

    def list_resource_templates(self) -> list[dict[str, Any]]:
        return [template.descriptor() for template in self._default_resource_templates()]

    def list_prompts(self) -> list[dict[str, Any]]:
        return [prompt.descriptor() for prompt in self._default_prompts()]

    def handle_jsonrpc(self, message: Any) -> dict[str, Any] | list[dict[str, Any]] | None:
        if isinstance(message, list):
            responses = [
                response
                for item in message
                if (response := self.handle_jsonrpc(item)) is not None
            ]
            return responses or None

        if not isinstance(message, dict):
            return _jsonrpc_error(None, INVALID_REQUEST, "request must be a JSON object")

        request_id = message.get("id")
        method = message.get("method")
        if not isinstance(method, str):
            return _jsonrpc_error(request_id, INVALID_REQUEST, "request method must be a string")

        if "id" not in message:
            self._handle_notification(method)
            return None

        try:
            result = self._handle_request(method, message.get("params") or {})
        except JsonRpcProtocolError as error:
            return _jsonrpc_error(request_id, error.code, error.message)
        except ValueError as error:
            return _jsonrpc_error(request_id, INVALID_PARAMS, str(error))
        except Exception as error:
            return _jsonrpc_error(request_id, INTERNAL_ERROR, str(error))

        return {"jsonrpc": "2.0", "id": request_id, "result": result}

    def handle_jsonrpc_line(self, line: str) -> dict[str, Any] | list[dict[str, Any]] | None:
        try:
            message = json.loads(line)
        except json.JSONDecodeError as error:
            return _jsonrpc_error(None, PARSE_ERROR, f"invalid JSON: {error.msg}")
        return self.handle_jsonrpc(message)

    def serve_stdio(
        self,
        input_stream: TextIO = sys.stdin,
        output_stream: TextIO = sys.stdout,
    ) -> None:
        for line in input_stream:
            if not line.strip():
                continue
            response = self.handle_jsonrpc_line(line)
            if response is None:
                continue
            output_stream.write(json.dumps(response, separators=(",", ":")) + "\n")
            output_stream.flush()

    def serve_http(self, host: str = "127.0.0.1", port: int = 47778, *, sse: bool = False) -> None:
        """Serve JSON-RPC over HTTP POST, with an optional MCP-style SSE endpoint."""

        mcp_server = self

        class Handler(http.server.BaseHTTPRequestHandler):
            server_version = f"{SERVER_NAME}/{SERVER_VERSION}"

            def do_GET(self) -> None:  # noqa: N802 - stdlib callback name
                if self.path in ("/", "/health", "/mcp"):
                    self._write_json(
                        {
                            "name": SERVER_NAME,
                            "version": SERVER_VERSION,
                            "transport": "sse" if sse else "http",
                            "jsonrpc_endpoint": "/mcp",
                            "sse_endpoint": "/sse",
                        }
                    )
                    return
                if self.path.startswith("/sse"):
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.send_header("Cache-Control", "no-cache")
                    self.send_header("Connection", "keep-alive")
                    self.end_headers()
                    self.wfile.write(b"event: endpoint\n")
                    self.wfile.write(b"data: /mcp\n\n")
                    self.wfile.write(b"event: tools\n")
                    self.wfile.write(
                        b"data: "
                        + json.dumps({"tools": mcp_server.list_tools()}, separators=(",", ":")).encode()
                        + b"\n\n"
                    )
                    self.wfile.flush()
                    return
                self.send_error(404, "not found")

            def do_POST(self) -> None:  # noqa: N802 - stdlib callback name
                if self.path not in ("/", "/mcp"):
                    self.send_error(404, "not found")
                    return
                try:
                    length = int(self.headers.get("Content-Length", "0"))
                except ValueError:
                    self.send_error(411, "invalid content length")
                    return
                payload = self.rfile.read(length).decode("utf-8")
                try:
                    message = json.loads(payload)
                except json.JSONDecodeError as error:
                    response: dict[str, Any] | list[dict[str, Any]] | None = _jsonrpc_error(
                        None, PARSE_ERROR, f"invalid JSON: {error.msg}"
                    )
                else:
                    response = mcp_server.handle_jsonrpc(message)
                if response is None:
                    self.send_response(202)
                    self.end_headers()
                    return
                self._write_json(response)

            def log_message(self, format: str, *args: Any) -> None:
                if mcp_server.log_level == "debug":
                    super().log_message(format, *args)

            def _write_json(self, payload: Any) -> None:
                body = json.dumps(payload, separators=(",", ":")).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        httpd = http.server.ThreadingHTTPServer((host, port), Handler)
        httpd.serve_forever()

    def _handle_notification(self, method: str) -> None:
        if method == "notifications/initialized":
            return

    def _handle_request(self, method: str, params: Any) -> dict[str, Any]:
        if method == "initialize":
            protocol_version = MCP_PROTOCOL_VERSION
            if isinstance(params, dict) and isinstance(params.get("protocolVersion"), str):
                protocol_version = params["protocolVersion"]
            return {
                "protocolVersion": protocol_version,
                "capabilities": {
                    "tools": {"listChanged": False},
                    "resources": {"subscribe": False, "listChanged": False},
                    "prompts": {"listChanged": False},
                    "logging": {},
                    "completions": {},
                },
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                "instructions": (
                    "PeekabooX exposes Linux desktop observation and action tools. "
                    "Real input remains gated by the daemon permission policy."
                ),
            }
        if method == "ping":
            return {}
        if method == "tools/list":
            return {"tools": self.list_tools()}
        if method == "tools/call":
            return self._handle_tools_call(params)
        if method == "resources/list":
            return {"resources": self.list_resources()}
        if method == "resources/read":
            return self._handle_resources_read(params)
        if method == "resources/templates/list":
            return {"resourceTemplates": self.list_resource_templates()}
        if method == "prompts/list":
            return {"prompts": self.list_prompts()}
        if method == "prompts/get":
            return self._handle_prompts_get(params)
        if method == "logging/setLevel":
            return self._handle_logging_set_level(params)
        if method == "completion/complete":
            return self._handle_completion_complete(params)
        raise JsonRpcProtocolError(METHOD_NOT_FOUND, f"unsupported MCP method: {method}")

    def _handle_tools_call(self, params: Any) -> dict[str, Any]:
        if not isinstance(params, dict):
            raise ValueError("tools/call params must be an object")
        name = params.get("name")
        if not isinstance(name, str) or not name:
            raise ValueError("tools/call params.name must be a non-empty string")
        if name not in self.tools:
            raise JsonRpcProtocolError(INVALID_PARAMS, f"unknown MCP tool: {name}")
        arguments = params.get("arguments") or {}
        if not isinstance(arguments, dict):
            raise ValueError("tools/call params.arguments must be an object")

        try:
            structured = self.call_tool(name, arguments)
            is_error = False
        except PreflightError as error:
            structured = _preflight_error_content(error, name)
            is_error = True
        except CapabilityDeniedError as error:
            structured = _capability_error_content(error, name)
            is_error = True
        except ConfirmationRequiredError as error:
            structured = _confirmation_error_content(error, name, required=True)
            is_error = True
        except ConfirmationDeniedError as error:
            structured = _confirmation_error_content(error, name, required=False)
            is_error = True
        except Exception as error:
            structured = _generic_error_content(error, name)
            is_error = True

        return {
            "content": _tool_result_content(structured),
            "structuredContent": structured,
            "isError": is_error,
        }

    def _handle_resources_read(self, params: Any) -> dict[str, Any]:
        if not isinstance(params, dict):
            raise ValueError("resources/read params must be an object")
        uri = params.get("uri")
        if not isinstance(uri, str) or not uri:
            raise ValueError("resources/read params.uri must be a non-empty string")
        content = self._read_resource(uri)
        return {"contents": [content]}

    def _handle_prompts_get(self, params: Any) -> dict[str, Any]:
        if not isinstance(params, dict):
            raise ValueError("prompts/get params must be an object")
        name = params.get("name")
        if not isinstance(name, str) or not name:
            raise ValueError("prompts/get params.name must be a non-empty string")
        arguments = params.get("arguments") or {}
        if not isinstance(arguments, dict):
            raise ValueError("prompts/get params.arguments must be an object")
        prompt = self._prompt_by_name(name)
        text = _render_prompt(prompt, arguments)
        return {
            "description": prompt.description,
            "messages": [
                {
                    "role": "user",
                    "content": {"type": "text", "text": text},
                }
            ],
        }

    def _handle_logging_set_level(self, params: Any) -> dict[str, Any]:
        if not isinstance(params, dict):
            raise ValueError("logging/setLevel params must be an object")
        level = params.get("level")
        if not isinstance(level, str):
            raise ValueError("logging/setLevel params.level must be a string")
        normalized = level.strip().lower()
        if normalized not in LOG_LEVELS:
            raise ValueError(f"logging level must be one of: {', '.join(LOG_LEVELS)}")
        self.log_level = normalized
        return {}

    def _handle_completion_complete(self, params: Any) -> dict[str, Any]:
        if not isinstance(params, dict):
            raise ValueError("completion/complete params must be an object")
        argument = params.get("argument")
        if not isinstance(argument, dict):
            raise ValueError("completion/complete params.argument must be an object")
        name = argument.get("name")
        value = argument.get("value", "")
        if not isinstance(name, str) or not name:
            raise ValueError("completion/complete argument.name must be a non-empty string")
        if not isinstance(value, str):
            raise ValueError("completion/complete argument.value must be a string")
        values = self._completion_values(name, params)
        matches = _completion_matches(values, value)
        return {
            "completion": {
                "values": matches[:100],
                "total": len(matches),
                "hasMore": len(matches) > 100,
            }
        }

    def _read_resource(self, uri: str) -> dict[str, Any]:
        if uri == "peekaboox://server/info":
            return _json_resource(uri, self._server_info())
        if uri == "peekaboox://tools":
            return _json_resource(uri, {"tools": self.list_tools()})
        if uri == "peekaboox://desktop/profiles":
            return _json_resource(uri, self._desktop_profiles({}))
        if uri == "peekaboox://doctor/latest":
            doctor = self.runtime._preflight_doctor_result if self.runtime is not None else None
            return _json_resource(uri, {"available": doctor is not None, "doctor": _to_mcp_value(doctor)})
        if uri == "peekaboox://preflight/latest":
            audits = self.runtime.preflight_audit() if self.runtime is not None else ()
            return _json_resource(
                uri,
                {
                    "available": bool(audits),
                    "preflight": _to_mcp_value(audits[-1]) if audits else None,
                },
            )
        if uri == "peekaboox://desktop/latest-snapshot":
            snapshot = self._require_runtime().latest_desktop_snapshot()
            return _json_resource(uri, {"available": snapshot is not None, "snapshot": _to_mcp_value(snapshot)})
        if uri == "peekaboox://desktop/graph/status":
            return _json_resource(uri, _to_mcp_value(self._require_runtime().desktop_graph_status()))
        if uri == "peekaboox://plugins":
            return _json_resource(uri, _to_mcp_value(self._require_runtime().list_plugins()))
        if uri == "peekaboox://audit/capabilities":
            audits = self.runtime.capability_audit() if self.runtime is not None else ()
            return _json_resource(uri, {"events": _to_mcp_value(audits)})
        if uri == "peekaboox://audit/confirmations":
            audits = self.runtime.confirmation_audit() if self.runtime is not None else ()
            return _json_resource(uri, {"events": _to_mcp_value(audits)})
        if uri == "peekaboox://audit/preflight":
            audits = self.runtime.preflight_audit() if self.runtime is not None else ()
            return _json_resource(uri, {"events": _to_mcp_value(audits)})
        if uri.startswith("peekaboox://docs/"):
            name = uri.removeprefix("peekaboox://docs/")
            path = DOC_RESOURCES.get(name)
            if path is None:
                raise JsonRpcProtocolError(INVALID_PARAMS, f"unknown MCP resource: {uri}")
            return _text_resource(uri, _read_repo_text(path), mime_type="text/markdown")
        raise JsonRpcProtocolError(INVALID_PARAMS, f"unknown MCP resource: {uri}")

    def _server_info(self) -> dict[str, Any]:
        return {
            "name": SERVER_NAME,
            "version": SERVER_VERSION,
            "protocol_version": MCP_PROTOCOL_VERSION,
            "log_level": self.log_level,
            "tool_count": len(self.tools),
            "resource_count": len(self._default_resources()),
            "prompt_count": len(self._default_prompts()),
            "capabilities": {
                "tools": True,
                "resources": True,
                "prompts": True,
                "logging": True,
                "completions": True,
            },
            "runtime": {
                "available": self.runtime is not None,
                "preflight_mode": self.runtime.preflight_mode if self.runtime is not None else None,
                "preflight_timeout_seconds": (
                    self.runtime.preflight_timeout_seconds if self.runtime is not None else None
                ),
            },
        }

    def _completion_values(self, name: str, params: dict[str, Any]) -> tuple[str, ...]:
        normalized = name.strip().lower().replace("-", "_")
        if normalized in {"tool", "tools", "tool_name", "name"}:
            return tuple(sorted(self.tools))
        if normalized in {"prompt", "prompt_name"}:
            return tuple(prompt.name for prompt in self._default_prompts())
        if normalized in {"resource", "uri", "resource_uri"}:
            return tuple(resource.uri for resource in self._default_resources())
        if normalized in {"app", "application"}:
            return self._desktop_profile_completion_values()
        if normalized in {"target", "desktop_target"}:
            app = _completion_context_value(params, "app")
            return self._desktop_target_completion_values(app)
        if normalized in {"category", "categories", "preflight_category"}:
            return PREFLIGHT_CATEGORIES
        if normalized in {"capability_profile", "profile"}:
            return KNOWN_CAPABILITY_PROFILES
        if normalized in {"workflow_action", "action"}:
            return WORKFLOW_ACTIONS
        if normalized == "format":
            return ("json", "yaml")
        if normalized == "level":
            return LOG_LEVELS
        return ()

    def _default_resources(self) -> tuple[McpResource, ...]:
        resources = [
            McpResource(
                uri="peekaboox://server/info",
                name="server-info",
                title="PeekabooX MCP Server Info",
                description="Server version, protocol capabilities, runtime status, and counts.",
            ),
            McpResource(
                uri="peekaboox://tools",
                name="tools",
                title="PeekabooX MCP Tools",
                description="Current MCP tool descriptors, schemas, annotations, and output schemas.",
            ),
            McpResource(
                uri="peekaboox://desktop/profiles",
                name="desktop-profiles",
                title="Desktop App Profiles",
                description="Supported desktop helper app profiles and target names.",
            ),
            McpResource(
                uri="peekaboox://doctor/latest",
                name="doctor-latest",
                title="Latest Doctor Result",
                description="Most recent Doctor result cached by runtime preflight or doctor calls.",
            ),
            McpResource(
                uri="peekaboox://preflight/latest",
                name="preflight-latest",
                title="Latest Preflight Decision",
                description="Most recent runtime preflight audit event.",
            ),
            McpResource(
                uri="peekaboox://desktop/latest-snapshot",
                name="desktop-latest-snapshot",
                title="Latest Desktop Graph Snapshot",
                description="Latest semantic desktop graph snapshot from the runtime memory store.",
            ),
            McpResource(
                uri="peekaboox://desktop/graph/status",
                name="desktop-graph-status",
                title="Desktop Graph Status",
                description="Semantic desktop graph freshness and invalidation status.",
            ),
            McpResource(
                uri="peekaboox://plugins",
                name="plugins",
                title="PeekabooX Plugins",
                description="Discovered local plugins and declared tools.",
            ),
            McpResource(
                uri="peekaboox://audit/capabilities",
                name="capability-audit",
                title="Capability Audit",
                description="In-memory capability audit events for this MCP runtime.",
            ),
            McpResource(
                uri="peekaboox://audit/confirmations",
                name="confirmation-audit",
                title="Confirmation Audit",
                description="In-memory confirmation audit events for this MCP runtime.",
            ),
            McpResource(
                uri="peekaboox://audit/preflight",
                name="preflight-audit",
                title="Preflight Audit",
                description="In-memory preflight audit events for this MCP runtime.",
            ),
        ]
        resources.extend(
            McpResource(
                uri=f"peekaboox://docs/{name}",
                name=f"docs-{name}",
                title=f"PeekabooX {name.title()} Docs",
                description=f"Repository documentation from {path}.",
                mime_type="text/markdown",
            )
            for name, path in DOC_RESOURCES.items()
        )
        return tuple(resources)

    def _default_resource_templates(self) -> tuple[McpResourceTemplate, ...]:
        return (
            McpResourceTemplate(
                uri_template="peekaboox://docs/{document}",
                name="docs",
                title="PeekabooX Documentation",
                description=(
                    "Read repository documentation; document is one of "
                    + ", ".join(sorted(DOC_RESOURCES))
                    + "."
                ),
            ),
            McpResourceTemplate(
                uri_template="peekaboox://audit/{kind}",
                name="audit",
                title="PeekabooX Runtime Audit",
                description="Read runtime audit events; kind is capabilities, confirmations, or preflight.",
                mime_type="application/json",
            ),
        )

    def _default_prompts(self) -> tuple[McpPrompt, ...]:
        return (
            McpPrompt(
                "diagnose-desktop",
                "Diagnose the current desktop/capture/input/OCR environment.",
                (
                    McpPromptArgument("problem", "Observed failure or symptom.", False),
                    McpPromptArgument("strict", "Whether the caller wants release-blocking checks.", False),
                ),
            ),
            McpPrompt(
                "safe-desktop-action",
                "Plan a gated desktop action using preflight, capability, and confirmation checks.",
                (
                    McpPromptArgument("goal", "The user-visible desktop goal.", True),
                    McpPromptArgument("app", "Optional desktop app profile.", False),
                    McpPromptArgument("target", "Optional app target name.", False),
                ),
            ),
            McpPrompt(
                "inspect-window",
                "Inspect a visible window before choosing capture, OCR, elements, or input tools.",
                (
                    McpPromptArgument("app", "Optional app id/name filter.", False),
                    McpPromptArgument("title", "Optional title substring or regex.", False),
                    McpPromptArgument("window_id", "Optional exact window id.", False),
                ),
            ),
            McpPrompt(
                "build-workflow",
                "Create an editable PeekabooX workflow plan from a goal.",
                (
                    McpPromptArgument("goal", "The workflow goal.", True),
                    McpPromptArgument("format", "json or yaml.", False),
                ),
            ),
            McpPrompt(
                "recover-from-tool-error",
                "Recover from a structured MCP tool error without parsing prose.",
                (
                    McpPromptArgument("error_json", "The MCP structuredContent error object.", True),
                ),
            ),
            McpPrompt(
                "plugin-development",
                "Design or validate a PeekabooX plugin SDK package.",
                (
                    McpPromptArgument("plugin_id", "Optional plugin id.", False),
                    McpPromptArgument("tool", "Optional plugin tool name.", False),
                ),
            ),
            McpPrompt(
                "ocr-visible-text",
                "Extract and verify visible text from a screen, window, or image.",
                (
                    McpPromptArgument("text_goal", "Text to find or verify.", False),
                    McpPromptArgument("app", "Optional desktop app scope.", False),
                ),
            ),
            McpPrompt(
                "semantic-click-plan",
                "Plan a semantic click using accessibility, graph cache, and vision fallback.",
                (
                    McpPromptArgument("selector", "Optional semantic selector.", False),
                    McpPromptArgument("app", "Optional app scope.", False),
                    McpPromptArgument("target", "Optional desktop helper target.", False),
                ),
            ),
        )

    def _prompt_by_name(self, name: str) -> McpPrompt:
        for prompt in self._default_prompts():
            if prompt.name == name:
                return prompt
        raise JsonRpcProtocolError(INVALID_PARAMS, f"unknown MCP prompt: {name}")

    def _default_tools(self) -> list[McpTool]:
        return [
            self._tool(
                "capture_screen",
                "Capture the current screen and optionally include semantic UI elements.",
                _schema(
                    {
                        "include_semantic_tree": {"type": "boolean", "default": False},
                        "region": RECT_SCHEMA,
                        "window_id": {"type": "string"},
                        "app": {"type": "string"},
                        "window_title": {"type": "string"},
                        "title_regex": {"type": "string"},
                    }
                ),
                self._capture_screen,
            ),
            self._tool(
                "capture_delta",
                "Capture a low-bandwidth screen delta for a persistent stream.",
                _schema(
                    {
                        "stream_id": {"type": "string", "default": "default"},
                        "reset": {"type": "boolean", "default": False},
                        "region": RECT_SCHEMA,
                        "window_id": {"type": "string"},
                        "per_channel_threshold": {"type": "integer", "minimum": 0, "maximum": 255},
                        "low_bandwidth": {"type": "boolean", "default": True},
                    }
                ),
                self._capture_delta,
            ),
            self._tool(
                "capture_backends",
                "Inspect and optionally probe screenshot and zero-copy capture backends.",
                _schema(
                    {
                        "output": {"type": "string", "default": "screenshot.png"},
                        "region": RECT_SCHEMA,
                        "diagnose": {"type": "boolean", "default": False},
                        "probe": {
                            "type": "string",
                            "enum": ["none", "file", "frame", "region", "dmabuf", "all"],
                            "default": "none",
                        },
                    }
                ),
                self._capture_backends,
            ),
            self._tool(
                "doctor",
                "Run PeekabooX environment diagnostics and return structured health checks.",
                _schema(
                    {
                        "strict": {"type": "boolean", "default": False},
                        "timeout_seconds": {"type": "number", "minimum": 0.1, "default": 30.0},
                    }
                ),
                self._doctor,
            ),
            self._tool(
                "preflight",
                "Check required Doctor categories before a live automation action.",
                _schema(
                    {
                        "categories": {
                            "type": "array",
                            "items": {
                                "type": "string",
                                "enum": ["desktop", "capture", "input", "ocr", "python"],
                            },
                            "minItems": 1,
                        },
                        "operation": {"type": "string", "default": "mcp"},
                        "require": {"type": "boolean", "default": False},
                        "refresh": {"type": "boolean", "default": False},
                        "timeout_seconds": {"type": "number", "minimum": 0.1, "default": 30.0},
                    },
                    required=["categories"],
                ),
                self._preflight,
            ),
            self._tool(
                "probe_dmabuf",
                "Probe the optional DMA-BUF capture/import path.",
                _schema(
                    {
                        "import_target": {
                            "type": "string",
                            "enum": ["compute", "egl", "egl_texture"],
                            "default": "compute",
                        },
                    }
                ),
                self._probe_dmabuf,
            ),
            self._tool(
                "click",
                "Click screen coordinates or a semantic selector.",
                _schema(
                    {
                        "x": {"type": "integer"},
                        "y": {"type": "integer"},
                        "selector": {"type": "string"},
                        "semantic_selector": {"type": "string"},
                        "region": RECT_SCHEMA,
                        "ratio_x": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                        "ratio_y": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                        "window_id": {"type": "string"},
                        "app": {"type": "string"},
                        "window_title": {"type": "string"},
                        "title_regex": {"type": "string"},
                        "button": {"type": "string", "enum": ["left", "middle", "right"]},
                        "dry_run": {"type": "boolean", "default": False},
                        "bounds_policy": {
                            "type": "string",
                            "enum": list(MOVE_BOUNDS_POLICY_CHOICES),
                        },
                        "backend": {"type": "string", "enum": list(MOVE_BACKEND_CHOICES)},
                        "restore": {"type": "boolean", "default": False},
                        "vision_fallback": {"type": "boolean", "default": False},
                    },
                    any_of=[
                        {"required": ["x", "y"]},
                        {"required": ["selector"]},
                        {"required": ["semantic_selector"]},
                        {"required": ["ratio_x", "ratio_y"]},
                        {"required": ["region"]},
                        {"required": ["window_id"]},
                        {"required": ["app"]},
                        {"required": ["window_title"]},
                        {"required": ["title_regex"]},
                    ],
                ),
                self._click,
            ),
            self._tool(
                "move_mouse",
                "Move the pointer through the daemon input backend.",
                _schema(
                    {
                        "x": {"type": "integer"},
                        "y": {"type": "integer"},
                        "relative_x": {"type": "integer"},
                        "relative_y": {"type": "integer"},
                        "region": RECT_SCHEMA,
                        "ratio_x": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                        "ratio_y": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                        "window_id": {"type": "string"},
                        "app": {"type": "string"},
                        "window_title": {"type": "string"},
                        "title_regex": {"type": "string"},
                        "dry_run": {"type": "boolean", "default": False},
                        "duration_ms": {"type": "integer", "minimum": 0},
                        "steps": {"type": "integer", "minimum": 1},
                        "bounds_policy": {
                            "type": "string",
                            "enum": list(MOVE_BOUNDS_POLICY_CHOICES),
                        },
                        "backend": {"type": "string", "enum": list(MOVE_BACKEND_CHOICES)},
                        "restore": {"type": "boolean", "default": False},
                    },
                    any_of=[
                        {"required": ["x", "y"]},
                        {"required": ["relative_x", "relative_y"]},
                        {"required": ["ratio_x", "ratio_y"]},
                        {"required": ["region"]},
                        {"required": ["window_id"]},
                        {"required": ["app"]},
                        {"required": ["window_title"]},
                        {"required": ["title_regex"]},
                    ],
                ),
                self._move_mouse,
            ),
            self._tool(
                "drag",
                "Drag from one absolute, current, or scoped-ratio endpoint to another through the daemon input backend.",
                _schema(
                    {
                        "from_x": {"type": "integer"},
                        "from_y": {"type": "integer"},
                        "to_x": {"type": "integer"},
                        "to_y": {"type": "integer"},
                        "from_current": {"type": "boolean", "default": False},
                        "from_ratio": {
                            "type": "array",
                            "items": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                            "minItems": 2,
                            "maxItems": 2,
                        },
                        "to_ratio": {
                            "type": "array",
                            "items": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                            "minItems": 2,
                            "maxItems": 2,
                        },
                        "region": RECT_SCHEMA,
                        "window_id": {"type": "string"},
                        "app": {"type": "string"},
                        "window_title": {"type": "string"},
                        "title_regex": {"type": "string"},
                        "button": {"type": "string", "enum": ["left", "middle", "right"]},
                        "duration_ms": {"type": "integer", "minimum": 0, "default": 250},
                        "steps": {"type": "integer", "minimum": 1},
                        "bounds_policy": {
                            "type": "string",
                            "enum": list(MOVE_BOUNDS_POLICY_CHOICES),
                        },
                        "backend": {"type": "string", "enum": list(DRAG_BACKEND_CHOICES)},
                        "restore": {"type": "boolean", "default": False},
                        "dry_run": {"type": "boolean", "default": False},
                    },
                    any_of=[
                        {"required": ["from_x", "from_y", "to_x", "to_y"]},
                        {"required": ["from_current", "to_x", "to_y"]},
                        {"required": ["from_ratio", "to_x", "to_y"]},
                        {"required": ["from_x", "from_y", "to_ratio"]},
                        {"required": ["from_current", "to_ratio"]},
                        {"required": ["from_ratio", "to_ratio"]},
                    ],
                ),
                self._drag,
            ),
            self._tool(
                "type_text",
                "Type text through the daemon input backend.",
                _schema(
                    {
                        "text": {"type": "string"},
                        "typing_speed_chars_per_second": {"type": "integer", "minimum": 1},
                        "dry_run": {"type": "boolean", "default": False},
                        "backend": {"type": "string", "enum": list(TYPE_BACKEND_CHOICES)},
                        "delay_ms": {"type": "integer", "minimum": 0},
                        "key_delay_ms": {"type": "integer", "minimum": 0},
                    },
                    required=["text"],
                ),
                self._type_text,
            ),
            self._tool(
                "paste_text",
                "Paste text through the daemon clipboard backend.",
                _schema(
                    {
                        "text": {"type": "string"},
                        "preserve_clipboard": {"type": "boolean", "default": False},
                        "dry_run": {"type": "boolean", "default": False},
                        "clipboard_backend": {
                            "type": "string",
                            "enum": ["auto", "wl-copy", "xclip", "xsel"],
                        },
                        "hotkey_backend": {
                            "type": "string",
                            "enum": ["auto", "ydotool", "xdotool"],
                        },
                        "delay_ms": {"type": "integer", "minimum": 0},
                        "restore_delay_ms": {"type": "integer", "minimum": 0},
                        "restore_policy": {
                            "type": "string",
                            "enum": ["strict", "best-effort", "off"],
                        },
                    },
                    required=["text"],
                ),
                self._paste_text,
            ),
            self._tool(
                "hotkey",
                "Press a keyboard shortcut through the daemon input backend.",
                _schema(
                    {
                        "keys": {
                            "type": "array",
                            "items": {"type": "string"},
                            "minItems": 1,
                        },
                        "dry_run": {"type": "boolean", "default": False},
                        "backend": {"type": "string", "enum": list(HOTKEY_BACKEND_CHOICES)},
                        "delay_ms": {"type": "integer", "minimum": 0},
                        "key_delay_ms": {"type": "integer", "minimum": 0},
                        "repeat": {"type": "integer", "minimum": 1},
                        "interval_ms": {"type": "integer", "minimum": 0},
                        "release_before": {"type": "boolean", "default": False},
                        "release_after": {"type": "boolean", "default": False},
                    },
                    required=["keys"],
                ),
                self._hotkey,
            ),
            self._tool(
                "find_element",
                "Find semantic UI elements by selector, optionally using vision fallback.",
                _schema(
                    {
                        "selector": {"type": "string"},
                        "vision_fallback": {"type": "boolean", "default": False},
                        "app": {"type": "string"},
                        "window_title": {"type": "string"},
                        "window_id": {"type": "string"},
                        "vision_region": RECT_SCHEMA,
                        "vision_edge_threshold": {"type": "integer", "minimum": 1, "maximum": 255},
                        "vision_min_width": {"type": "integer", "minimum": 1},
                        "vision_min_height": {"type": "integer", "minimum": 1},
                        "vision_min_component_pixels": {"type": "integer", "minimum": 1},
                        "vision_max_elements": {"type": "integer", "minimum": 1},
                        "vision_merge_distance": {"type": "integer", "minimum": 0},
                    },
                    required=["selector"],
                ),
                self._find_element,
            ),
            self._tool(
                "list_windows",
                "List, filter, and optionally diagnose visible desktop windows.",
                _schema(
                    {
                        "id": {"type": "string"},
                        "app": {"type": "string"},
                        "title": {"type": "string"},
                        "title_regex": {"type": "string"},
                        "focused": {"type": "boolean", "default": False},
                        "limit": {"type": "integer", "minimum": 1},
                        "sort": {"type": "string", "enum": list(WINDOW_SORT_CHOICES)},
                        "backend": {"type": "string", "enum": list(WINDOW_BACKEND_CHOICES)},
                        "diagnose": {"type": "boolean", "default": False},
                    }
                ),
                self._list_windows,
            ),
            self._tool(
                "desktop_focus",
                "Focus or launch a supported desktop application.",
                _schema(
                    {
                        "app": {"type": "string"},
                        "use_gnome_overview": {"type": "boolean", "default": True},
                        "launch_if_needed": {"type": "boolean", "default": True},
                        "wait_after_focus_ms": {"type": "integer", "minimum": 0, "default": 1000},
                        "overview_wait_ms": {"type": "integer", "minimum": 0, "default": 800},
                        "window_title": {"type": "string"},
                        "window_id": {"type": "string"},
                        "verify": {"type": "boolean", "default": False},
                    },
                    required=["app"],
                ),
                self._desktop_focus,
            ),
            self._tool(
                "desktop_locate",
                "Resolve a named app target to screen coordinates.",
                _schema(
                    {
                        "app": {"type": "string"},
                        "target": {"type": "string"},
                        "image_path": {"type": "string"},
                        "prefer_accessibility": {"type": "boolean", "default": True},
                        "window_title": {"type": "string"},
                        "window_id": {"type": "string"},
                    },
                    required=["app", "target"],
                ),
                self._desktop_locate,
            ),
            self._tool(
                "desktop_click",
                "Click a named target inside a supported desktop application.",
                _schema(
                    {
                        "app": {"type": "string"},
                        "target": {"type": "string"},
                        "image_path": {"type": "string"},
                        "prefer_accessibility": {"type": "boolean", "default": True},
                        "window_title": {"type": "string"},
                        "window_id": {"type": "string"},
                        "button": {"type": "string", "enum": ["left", "middle", "right"]},
                        "dry_run": {"type": "boolean", "default": False},
                        "verify": {"type": "boolean", "default": False},
                    },
                    required=["app", "target"],
                ),
                self._desktop_click,
            ),
            self._tool(
                "desktop_drag",
                "Drag inside a named app target using rectangle-relative ratios.",
                _schema(
                    {
                        "app": {"type": "string"},
                        "target": {"type": "string"},
                        "image_path": {"type": "string"},
                        "prefer_accessibility": {"type": "boolean", "default": True},
                        "window_title": {"type": "string"},
                        "window_id": {"type": "string"},
                        "button": {"type": "string", "enum": ["left", "middle", "right"]},
                        "from_ratio": {
                            "type": "array",
                            "items": {"type": "number", "minimum": 0, "maximum": 1},
                            "minItems": 2,
                            "maxItems": 2,
                        },
                        "to_ratio": {
                            "type": "array",
                            "items": {"type": "number", "minimum": 0, "maximum": 1},
                            "minItems": 2,
                            "maxItems": 2,
                        },
                        "duration_ms": {"type": "integer", "minimum": 0, "default": 250},
                        "dry_run": {"type": "boolean", "default": False},
                        "verify": {"type": "boolean", "default": False},
                    },
                    required=["app", "target"],
                ),
                self._desktop_drag,
            ),
            self._tool(
                "desktop_type_into",
                "Type text into a named target inside a supported desktop application.",
                _schema(
                    {
                        "app": {"type": "string"},
                        "target": {"type": "string"},
                        "text": {"type": "string"},
                        "image_path": {"type": "string"},
                        "prefer_accessibility": {"type": "boolean", "default": True},
                        "window_title": {"type": "string"},
                        "window_id": {"type": "string"},
                        "clear": {"type": "boolean", "default": False},
                        "dry_run": {"type": "boolean", "default": False},
                        "verify": {"type": "boolean", "default": False},
                    },
                    required=["app", "target", "text"],
                ),
                self._desktop_type_into,
            ),
            self._tool(
                "desktop_assert",
                "Assert that a named target is present, active, or contains text.",
                _schema(
                    {
                        "app": {"type": "string"},
                        "target": {"type": "string"},
                        "assertion": {
                            "type": "string",
                            "enum": [
                                "present",
                                "not_present",
                                "active",
                                "not_active",
                                "contains",
                                "not_contains",
                            ],
                            "default": "present",
                        },
                        "expected_text": {"type": "string"},
                        "image_path": {"type": "string"},
                        "prefer_accessibility": {"type": "boolean", "default": True},
                        "window_title": {"type": "string"},
                        "window_id": {"type": "string"},
                    },
                    required=["app", "target"],
                ),
                self._desktop_assert,
            ),
            self._tool(
                "list_plugins",
                "List installed PeekabooX plugins and declared tools.",
                _schema(
                    {
                        "paths": {
                            "type": "array",
                            "items": {"type": "string"},
                        },
                    }
                ),
                self._list_plugins,
            ),
            self._tool(
                "call_plugin_tool",
                "Execute a declared PeekabooX process plugin tool.",
                _schema(
                    {
                        "plugin_id": {"type": "string"},
                        "tool": {"type": "string"},
                        "arguments": {"type": "object"},
                        "paths": {
                            "type": "array",
                            "items": {"type": "string"},
                        },
                        "timeout_seconds": {"type": "number", "minimum": 0.1},
                        "max_output_bytes": {"type": "integer", "minimum": 0},
                    },
                    required=["plugin_id", "tool"],
                ),
                self._call_plugin_tool,
            ),
            self._tool(
                "get_desktop_state",
                "Return active window, windows, and semantic UI elements.",
                _schema({}),
                self._get_desktop_state,
            ),
            self._tool(
                "ingest_desktop_snapshot",
                "Sample current desktop state and store it in the semantic desktop graph.",
                _schema(
                    {
                        "snapshot_id": {"type": "string"},
                    }
                ),
                self._ingest_desktop_snapshot,
            ),
            self._tool(
                "latest_desktop_snapshot",
                "Return the latest semantic desktop graph snapshot, if one exists.",
                _schema({}),
                self._latest_desktop_snapshot,
            ),
            self._tool(
                "record_desktop_event",
                "Record a desktop event and invalidate the semantic desktop graph.",
                _schema(
                    {
                        "kind": {"type": "string"},
                        "source": {"type": "string"},
                        "target_id": {"type": "string"},
                        "payload": {"type": "object"},
                        "occurred_at_unix_ms": {"type": "integer"},
                    },
                    required=["kind"],
                ),
                self._record_desktop_event,
            ),
            self._tool(
                "desktop_graph_status",
                "Return semantic desktop graph cache status and invalidation metadata.",
                _schema({}),
                self._desktop_graph_status,
            ),
            self._tool(
                "refresh_desktop_graph",
                "Sample current desktop state and refresh the semantic desktop graph.",
                _schema(
                    {
                        "snapshot_id": {"type": "string"},
                    }
                ),
                self._refresh_desktop_graph,
            ),
            self._tool(
                "query_desktop_graph",
                "Query nodes from the runtime semantic desktop graph.",
                _schema(
                    {
                        "kind": {"type": "string"},
                        "label_contains": {"type": "string"},
                        "role": {"type": "string"},
                        "attribute_equals": {"type": "object"},
                        "contained_by": {"type": "string"},
                        "latest_only": {"type": "boolean", "default": True},
                        "refresh_if_stale": {"type": "boolean", "default": False},
                    }
                ),
                self._query_desktop_graph,
            ),
            self._tool(
                "execute_goal",
                "Plan and execute a goal through the AgentRuntime retry/verification loop.",
                _schema(
                    {
                        "goal": {"type": "string"},
                        "replan_on_failure": {"type": "boolean", "default": True},
                        "max_replans": {"type": "integer", "minimum": 0, "default": 1},
                    },
                    required=["goal"],
                ),
                self._execute_goal,
            ),
            self._tool(
                "generate_workflow",
                (
                    "Generate an editable workflow draft from a goal and optional "
                    "desktop graph context."
                ),
                _schema(
                    {
                        "goal": {"type": "string"},
                        "refresh_desktop_graph": {"type": "boolean", "default": False},
                        "format": {"type": "string", "enum": ["json", "yaml"]},
                    },
                    required=["goal"],
                ),
                self._generate_workflow,
            ),
            self._tool(
                "save_generated_workflow",
                "Generate and save an editable workflow draft as JSON or YAML.",
                _schema(
                    {
                        "goal": {"type": "string"},
                        "path": {"type": "string"},
                        "refresh_desktop_graph": {"type": "boolean", "default": False},
                        "format": {"type": "string", "enum": ["json", "yaml"]},
                    },
                    required=["goal", "path"],
                ),
                self._save_generated_workflow,
            ),
            self._tool(
                "refine_workflow",
                "Refine a workflow draft through the configured structured workflow provider.",
                _schema(
                    {
                        "goal": {"type": "string"},
                        "workflow": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "steps": {
                                    "type": "array",
                                    "items": WORKFLOW_STEP_SCHEMA,
                                    "minItems": 1,
                                },
                            },
                            "required": ["steps"],
                            "additionalProperties": False,
                        },
                        "refresh_desktop_graph": {"type": "boolean", "default": False},
                        "format": {"type": "string", "enum": ["json", "yaml"]},
                    },
                    required=["goal"],
                ),
                self._refine_workflow,
            ),
            self._tool(
                "save_refined_workflow",
                "Refine and save a workflow draft as JSON or YAML.",
                _schema(
                    {
                        "goal": {"type": "string"},
                        "path": {"type": "string"},
                        "workflow": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "steps": {
                                    "type": "array",
                                    "items": WORKFLOW_STEP_SCHEMA,
                                    "minItems": 1,
                                },
                            },
                            "required": ["steps"],
                            "additionalProperties": False,
                        },
                        "refresh_desktop_graph": {"type": "boolean", "default": False},
                        "format": {"type": "string", "enum": ["json", "yaml"]},
                    },
                    required=["goal", "path"],
                ),
                self._save_refined_workflow,
            ),
            self._tool(
                "execute_workflow",
                (
                    "Execute explicit workflow steps with retries, verification, "
                    "and recovery metadata."
                ),
                _schema(
                    {
                        "name": {"type": "string"},
                        "steps": {
                            "type": "array",
                            "items": WORKFLOW_STEP_SCHEMA,
                            "minItems": 1,
                        },
                    },
                    required=["steps"],
                ),
                self._execute_workflow,
            ),
            self._tool(
                "execute_workflow_file",
                "Load and execute a JSON or YAML workflow file.",
                _schema(
                    {
                        "path": {"type": "string"},
                    },
                    required=["path"],
                ),
                self._execute_workflow_file,
            ),
            self._tool(
                "start_workflow_recording",
                "Start recording subsequent runtime actions as workflow steps.",
                _schema(
                    {
                        "name": {"type": "string"},
                    }
                ),
                self._start_workflow_recording,
            ),
            self._tool(
                "stop_workflow_recording",
                "Stop the active workflow recording and return the recorded workflow.",
                _schema({}),
                self._stop_workflow_recording,
            ),
            self._tool(
                "get_recorded_workflow",
                "Return the active or most recently completed workflow recording.",
                _schema({}),
                self._get_recorded_workflow,
            ),
            self._tool(
                "save_recorded_workflow",
                "Save the active or most recently completed workflow recording as JSON or YAML.",
                _schema(
                    {
                        "path": {"type": "string"},
                        "format": {"type": "string", "enum": ["json", "yaml"]},
                    },
                    required=["path"],
                ),
                self._save_recorded_workflow,
            ),
            self._tool(
                "ocr_screen",
                "Run OCR on the full screen or a region.",
                _schema(
                    {
                        "region": RECT_SCHEMA,
                        "language": {"type": "string"},
                        "image_path": {"type": "string"},
                        "window_id": {"type": "string"},
                        "window_title": {"type": "string"},
                        "app": {"type": "string"},
                        "page_segmentation_mode": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 13,
                        },
                        "engine_mode": {"type": "integer", "minimum": 0, "maximum": 3},
                        "dpi": {"type": "integer", "minimum": 1},
                        "min_confidence": {
                            "type": "number",
                            "minimum": 0.0,
                            "maximum": 1.0,
                        },
                        "whitelist": {"type": "string"},
                        "config": {
                            "type": "array",
                            "items": {"type": "string"},
                        },
                        "scale": {"type": "number", "minimum": 0.1, "maximum": 8.0},
                        "grayscale": {"type": "boolean", "default": False},
                        "threshold": {"type": "integer", "minimum": 0, "maximum": 255},
                        "invert": {"type": "boolean", "default": False},
                        "contrast": {
                            "type": "number",
                            "minimum": -255.0,
                            "maximum": 255.0,
                        },
                        "deskew": {"type": "boolean", "default": False},
                    }
                ),
                self._ocr_screen,
            ),
            self._tool(
                "compare_images",
                "Compare two image files and return visual diff metadata.",
                _schema(
                    {
                        "expected_path": {"type": "string"},
                        "actual_path": {"type": "string"},
                        "region": RECT_SCHEMA,
                        "ignore_regions": {
                            "type": "array",
                            "items": RECT_SCHEMA,
                            "default": [],
                        },
                        "per_channel_threshold": {"type": "integer", "minimum": 0},
                        "max_changed_ratio": {"type": "number", "minimum": 0, "maximum": 1},
                        "max_changed_pixels": {"type": "integer", "minimum": 0},
                        "max_mean_absolute_error": {
                            "type": "number",
                            "minimum": 0,
                            "maximum": 255,
                        },
                        "max_channel_delta": {"type": "integer", "minimum": 0, "maximum": 255},
                        "size_policy": {
                            "type": "string",
                            "enum": ["error", "common-region", "resize-actual"],
                            "default": "error",
                        },
                        "alpha": {
                            "type": "string",
                            "enum": ["ignore", "compare"],
                            "default": "ignore",
                        },
                    },
                    required=["expected_path", "actual_path"],
                ),
                self._compare_images,
            ),
            self._tool(
                "detect_ui_state",
                "Classify an image sequence as stable, loading, or changing.",
                _schema(
                    {
                        "image_paths": {
                            "type": "array",
                            "items": {"type": "string"},
                            "minItems": 2,
                        },
                        "region": RECT_SCHEMA,
                        "ignore_regions": {
                            "type": "array",
                            "items": RECT_SCHEMA,
                            "default": [],
                        },
                        "per_channel_threshold": {"type": "integer", "minimum": 0},
                        "stable_max_changed_ratio": {"type": "number", "minimum": 0, "maximum": 1},
                        "stable_max_changed_pixels": {"type": "integer", "minimum": 0},
                        "stable_max_mean_absolute_error": {
                            "type": "number",
                            "minimum": 0,
                            "maximum": 255,
                        },
                        "stable_max_channel_delta": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 255,
                        },
                        "loading_min_changed_ratio": {"type": "number", "minimum": 0, "maximum": 1},
                        "loading_min_changed_pixels": {"type": "integer", "minimum": 0},
                        "required_stable_transitions": {"type": "integer", "minimum": 1},
                        "size_policy": {
                            "type": "string",
                            "enum": ["error", "common-region", "resize-actual"],
                            "default": "error",
                        },
                        "alpha": {
                            "type": "string",
                            "enum": ["ignore", "compare"],
                            "default": "ignore",
                        },
                    },
                    required=["image_paths"],
                ),
                self._detect_ui_state,
            ),
            self._tool(
                "detect_ui_elements",
                "Detect visible UI-like regions in an image file.",
                _schema(
                    {
                        "image_path": {"type": "string"},
                        "region": RECT_SCHEMA,
                        "ignore_regions": {
                            "type": "array",
                            "items": RECT_SCHEMA,
                        },
                        "edge_threshold": {"type": "integer", "minimum": 1},
                        "min_width": {"type": "integer", "minimum": 1},
                        "min_height": {"type": "integer", "minimum": 1},
                        "min_component_pixels": {"type": "integer", "minimum": 1},
                        "min_confidence": {
                            "type": "number",
                            "minimum": 0.0,
                            "maximum": 1.0,
                        },
                        "max_width": {"type": "integer", "minimum": 1},
                        "max_height": {"type": "integer", "minimum": 1},
                        "min_area": {"type": "integer", "minimum": 1},
                        "max_area": {"type": "integer", "minimum": 1},
                        "max_elements": {"type": "integer", "minimum": 1},
                        "merge_distance": {"type": "integer", "minimum": 0},
                        "padding": {"type": "integer", "minimum": 0},
                        "sort": {
                            "type": "string",
                            "enum": ["position", "area", "confidence"],
                            "default": "position",
                        },
                        "mask_output_path": {"type": "string"},
                        "overlay_output_path": {"type": "string"},
                    },
                    required=["image_path"],
                ),
                self._detect_ui_elements,
            ),
            self._tool(
                "find_elements",
                "Find semantic UI elements by selector and optionally limit the result count.",
                _schema(
                    {
                        "selector": {"type": "string"},
                        "limit": {"type": "integer", "minimum": 1},
                        "vision_fallback": {"type": "boolean", "default": False},
                        "app": {"type": "string"},
                        "window_title": {"type": "string"},
                        "window_id": {"type": "string"},
                        "vision_region": RECT_SCHEMA,
                        "vision_edge_threshold": {"type": "integer", "minimum": 1, "maximum": 255},
                        "vision_min_width": {"type": "integer", "minimum": 1},
                        "vision_min_height": {"type": "integer", "minimum": 1},
                        "vision_min_component_pixels": {"type": "integer", "minimum": 1},
                        "vision_max_elements": {"type": "integer", "minimum": 1},
                        "vision_merge_distance": {"type": "integer", "minimum": 0},
                    },
                    required=["selector"],
                ),
                self._find_elements,
            ),
            self._tool(
                "elements",
                "CLI-compatible alias for find_elements.",
                _schema(
                    {
                        "selector": {"type": "string"},
                        "limit": {"type": "integer", "minimum": 1},
                        "vision_fallback": {"type": "boolean", "default": False},
                        "app": {"type": "string"},
                        "window_title": {"type": "string"},
                        "window_id": {"type": "string"},
                    },
                    required=["selector"],
                ),
                self._find_elements,
            ),
            self._tool(
                "vision_elements",
                "CLI-compatible alias for detect_ui_elements.",
                _schema(
                    {
                        "image_path": {"type": "string"},
                        "region": RECT_SCHEMA,
                        "ignore_regions": {
                            "type": "array",
                            "items": RECT_SCHEMA,
                        },
                        "edge_threshold": {"type": "integer", "minimum": 1},
                        "min_width": {"type": "integer", "minimum": 1},
                        "min_height": {"type": "integer", "minimum": 1},
                        "min_component_pixels": {"type": "integer", "minimum": 1},
                        "min_confidence": {
                            "type": "number",
                            "minimum": 0.0,
                            "maximum": 1.0,
                        },
                        "max_width": {"type": "integer", "minimum": 1},
                        "max_height": {"type": "integer", "minimum": 1},
                        "min_area": {"type": "integer", "minimum": 1},
                        "max_area": {"type": "integer", "minimum": 1},
                        "max_elements": {"type": "integer", "minimum": 1},
                        "merge_distance": {"type": "integer", "minimum": 0},
                        "padding": {"type": "integer", "minimum": 0},
                        "sort": {
                            "type": "string",
                            "enum": ["position", "area", "confidence"],
                            "default": "position",
                        },
                        "mask_output_path": {"type": "string"},
                        "overlay_output_path": {"type": "string"},
                    },
                    required=["image_path"],
                ),
                self._detect_ui_elements,
            ),
            self._tool(
                "ocr",
                "CLI-compatible OCR alias for screen, window, region, or image OCR.",
                _ocr_input_schema(),
                self._ocr_screen,
            ),
            self._tool(
                "ocr_image",
                "Run OCR over an existing image file.",
                _ocr_input_schema(),
                self._ocr_screen,
            ),
            self._tool(
                "capture_dmabuf",
                "CLI-compatible alias for the DMA-BUF capture/import probe.",
                _schema(
                    {
                        "import_target": {
                            "type": "string",
                            "enum": ["compute", "egl", "egl_texture"],
                            "default": "compute",
                        },
                    }
                ),
                self._probe_dmabuf,
            ),
            self._tool(
                "desktop_profiles",
                "List supported desktop helper app profiles, launch commands, target metadata, and availability.",
                _schema(
                    {
                        "app": {"type": "string"},
                        "target": {"type": "string"},
                        "command": {"type": "string"},
                        "desktop_id": {"type": "string"},
                        "supports": {"type": "string"},
                        "check": {"type": "boolean", "default": False},
                        "installed": {"type": "boolean", "default": False},
                        "available": {"type": "boolean", "default": False},
                    }
                ),
                self._desktop_profiles,
            ),
            self._tool(
                "plan",
                "Decompose a goal into high-level planning steps.",
                _schema({"goal": {"type": "string"}}, required=["goal"]),
                self._plan,
            ),
            self._tool(
                "plan_workflow",
                "Create a simple workflow draft from a goal.",
                _schema(
                    {
                        "goal": {"type": "string"},
                        "format": {"type": "string", "enum": ["json", "yaml"]},
                    },
                    required=["goal"],
                ),
                self._plan_workflow,
            ),
            self._tool(
                "replan_workflow",
                "Generate a replacement workflow after a failed workflow result.",
                _schema(
                    {
                        "goal": {"type": "string"},
                        "failed_workflow": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "steps": {
                                    "type": "array",
                                    "items": WORKFLOW_STEP_SCHEMA,
                                    "minItems": 1,
                                },
                            },
                            "required": ["steps"],
                            "additionalProperties": False,
                        },
                        "failed_result": {"type": "object"},
                        "refresh_desktop_graph": {"type": "boolean", "default": False},
                        "format": {"type": "string", "enum": ["json", "yaml"]},
                    },
                    required=["goal", "failed_workflow", "failed_result"],
                ),
                self._replan_workflow,
            ),
            self._tool(
                "load_workflow_file",
                "Load a JSON or YAML workflow file without executing it.",
                _schema({"path": {"type": "string"}}, required=["path"]),
                self._load_workflow_file,
            ),
            self._tool(
                "query_desktop_edges",
                "Query edges from the runtime semantic desktop graph.",
                _schema(
                    {
                        "source": {"type": "string"},
                        "target": {"type": "string"},
                        "kind": {"type": "string"},
                        "latest_only": {"type": "boolean", "default": True},
                    }
                ),
                self._query_desktop_edges,
            ),
            self._tool(
                "capability_audit",
                "Return in-memory runtime capability audit events.",
                _schema({}),
                self._capability_audit,
            ),
            self._tool(
                "confirmation_audit",
                "Return in-memory runtime confirmation audit events.",
                _schema({}),
                self._confirmation_audit,
            ),
            self._tool(
                "preflight_audit",
                "Return in-memory runtime preflight audit events.",
                _schema({}),
                self._preflight_audit,
            ),
        ]

    def _tool(
        self,
        name: str,
        description: str,
        input_schema: dict[str, Any],
        handler: Callable[[dict[str, Any]], Any],
    ) -> McpTool:
        return McpTool(
            name=name,
            description=description,
            input_schema=input_schema,
            handler=handler if self.runtime is not None else None,
            title=_tool_title(name),
            output_schema=_tool_output_schema(name),
            annotations=_tool_annotations(name),
        )

    def _require_runtime(self) -> AgentRuntime:
        if self.runtime is None:
            raise RuntimeError("McpServer requires an AgentRuntime to execute tools")
        return self.runtime

    def _capture_screen(self, arguments: dict[str, Any]) -> dict[str, Any]:
        result = self._require_runtime().capture_screen(
            include_semantic_tree=bool(arguments.get("include_semantic_tree", False)),
            region=_optional_rect(arguments, "region"),
            window_id=_optional_string(arguments, "window_id"),
            app=_optional_string(arguments, "app"),
            window_title=_optional_string(arguments, "window_title"),
            title_regex=_optional_string(arguments, "title_regex"),
        )
        payload = _to_mcp_value(result)
        payload["image_base64"] = payload.pop("image")
        return payload

    def _capture_delta(self, arguments: dict[str, Any]) -> dict[str, Any]:
        result = self._require_runtime().capture_delta(
            stream_id=_optional_string(arguments, "stream_id") or "default",
            reset=bool(arguments.get("reset", False)),
            region=_optional_rect(arguments, "region"),
            window_id=_optional_string(arguments, "window_id"),
            per_channel_threshold=_optional_int(arguments, "per_channel_threshold"),
            low_bandwidth=bool(arguments.get("low_bandwidth", True)),
        )
        payload = _to_mcp_value(result)
        payload["patch_base64"] = payload.pop("patch")
        return payload

    def _capture_backends(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().capture_backends(
                output=_optional_string(arguments, "output") or "screenshot.png",
                region=_optional_rect(arguments, "region"),
                diagnose=_optional_bool(arguments, "diagnose"),
                probe=_optional_choice(
                    arguments,
                    "probe",
                    ("none", "file", "frame", "region", "dmabuf", "all"),
                )
                or "none",
            )
        )

    def _doctor(self, arguments: dict[str, Any]) -> dict[str, Any]:
        timeout = _optional_float(arguments, "timeout_seconds")
        if timeout is not None and timeout <= 0:
            raise ValueError("timeout_seconds must be greater than zero")
        return _to_mcp_value(
            self._require_runtime().doctor(
                strict=_optional_bool(arguments, "strict"),
                timeout_seconds=timeout or 30.0,
            )
        )

    def _preflight(self, arguments: dict[str, Any]) -> dict[str, Any]:
        timeout = _optional_float(arguments, "timeout_seconds")
        if timeout is not None and timeout <= 0:
            raise ValueError("timeout_seconds must be greater than zero")
        categories = _required_string_array(arguments, "categories")
        runtime = self._require_runtime()
        if _optional_bool(arguments, "require"):
            result = runtime.require_preflight(
                categories,
                operation=_optional_string(arguments, "operation") or "mcp",
                refresh=_optional_bool(arguments, "refresh"),
                timeout_seconds=timeout,
            )
        else:
            result = runtime.preflight(
                categories,
                operation=_optional_string(arguments, "operation") or "mcp",
                refresh=_optional_bool(arguments, "refresh"),
                timeout_seconds=timeout,
            )
        return _to_mcp_value(result)

    def _probe_dmabuf(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().probe_dmabuf(
                _optional_string(arguments, "import_target") or "compute"
            )
        )

    def _click(self, arguments: dict[str, Any]) -> dict[str, Any]:
        runtime = self._require_runtime()
        vision_fallback = bool(arguments.get("vision_fallback", False))
        selector = arguments.get("semantic_selector", arguments.get("selector"))
        has_coordinates = "x" in arguments or "y" in arguments
        region = _optional_rect(arguments, "region")
        ratio_x = _optional_float(arguments, "ratio_x")
        ratio_y = _optional_float(arguments, "ratio_y")
        window_id = _optional_string(arguments, "window_id")
        app = _optional_string(arguments, "app")
        window_title = _optional_string(arguments, "window_title")
        title_regex = _optional_string(arguments, "title_regex")
        has_scope = any(
            value is not None
            for value in (region, ratio_x, ratio_y, window_id, app, window_title, title_regex)
        )
        common_kwargs = {
            "button": _optional_string(arguments, "button") or "left",
            "dry_run": _optional_bool(arguments, "dry_run"),
            "bounds_policy": _optional_choice(
                arguments, "bounds_policy", MOVE_BOUNDS_POLICY_CHOICES
            ),
            "backend": _optional_choice(arguments, "backend", MOVE_BACKEND_CHOICES),
            "restore": _optional_bool(arguments, "restore"),
        }

        if selector is not None:
            if has_coordinates or has_scope:
                raise ValueError("provide exactly one click target")
            return _to_mcp_value(
                runtime.click_selector(
                    str(selector),
                    vision_fallback=vision_fallback,
                    **common_kwargs,
                )
            )

        if has_coordinates and ("x" not in arguments or "y" not in arguments):
            raise ValueError("click x/y target is incomplete")
        if not has_coordinates and not has_scope:
            raise ValueError("click requires x/y coordinates, selector, or scope")
        return _to_mcp_value(
            runtime.click(
                x=int(arguments["x"]) if has_coordinates else None,
                y=int(arguments["y"]) if has_coordinates else None,
                vision_fallback=vision_fallback,
                region=region,
                ratio_x=ratio_x,
                ratio_y=ratio_y,
                window_id=window_id,
                app=app,
                window_title=window_title,
                title_regex=title_regex,
                **common_kwargs,
            )
        )

    def _move_mouse(self, arguments: dict[str, Any]) -> dict[str, Any]:
        region = _optional_rect(arguments, "region")
        return _to_mcp_value(
            self._require_runtime().move_mouse(
                _optional_int(arguments, "x"),
                _optional_int(arguments, "y"),
                relative_x=_optional_int(arguments, "relative_x"),
                relative_y=_optional_int(arguments, "relative_y"),
                region=region,
                ratio_x=_optional_float(arguments, "ratio_x"),
                ratio_y=_optional_float(arguments, "ratio_y"),
                window_id=_optional_string(arguments, "window_id"),
                app=_optional_string(arguments, "app"),
                window_title=_optional_string(arguments, "window_title"),
                title_regex=_optional_string(arguments, "title_regex"),
                dry_run=_optional_bool(arguments, "dry_run"),
                duration_ms=_optional_int(arguments, "duration_ms"),
                steps=_optional_positive_int(arguments, "steps"),
                bounds_policy=_optional_choice(
                    arguments, "bounds_policy", MOVE_BOUNDS_POLICY_CHOICES
                ),
                backend=_optional_choice(arguments, "backend", MOVE_BACKEND_CHOICES),
                restore=_optional_bool(arguments, "restore"),
            )
        )

    def _drag(self, arguments: dict[str, Any]) -> dict[str, Any]:
        duration_ms = int(arguments.get("duration_ms", 250))
        if duration_ms < 0:
            raise ValueError("duration_ms must be non-negative")
        return _to_mcp_value(
            self._require_runtime().drag(
                _optional_int(arguments, "from_x"),
                _optional_int(arguments, "from_y"),
                _optional_int(arguments, "to_x"),
                _optional_int(arguments, "to_y"),
                button=_optional_string(arguments, "button") or "left",
                duration_ms=duration_ms,
                dry_run=_optional_bool(arguments, "dry_run"),
                steps=_optional_positive_int(arguments, "steps"),
                bounds_policy=_optional_choice(
                    arguments, "bounds_policy", MOVE_BOUNDS_POLICY_CHOICES
                ),
                backend=_optional_choice(arguments, "backend", DRAG_BACKEND_CHOICES),
                restore=_optional_bool(arguments, "restore"),
                from_current=_optional_bool(arguments, "from_current"),
                from_ratio=_optional_ratio_pair(arguments, "from_ratio"),
                to_ratio=_optional_ratio_pair(arguments, "to_ratio"),
                region=_optional_rect(arguments, "region"),
                window_id=_optional_string(arguments, "window_id"),
                app=_optional_string(arguments, "app"),
                window_title=_optional_string(arguments, "window_title"),
                title_regex=_optional_string(arguments, "title_regex"),
            )
        )

    def _type_text(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().type_text(
                _required_str(arguments, "text"),
                typing_speed_chars_per_second=_optional_positive_int(
                    arguments, "typing_speed_chars_per_second"
                ),
                dry_run=_optional_bool(arguments, "dry_run"),
                backend=_optional_choice(arguments, "backend", TYPE_BACKEND_CHOICES),
                delay_ms=_optional_nonnegative_int(arguments, "delay_ms"),
                key_delay_ms=_optional_nonnegative_int(arguments, "key_delay_ms"),
            )
        )

    def _paste_text(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().paste_text(
                _required_str(arguments, "text"),
                preserve_clipboard=bool(arguments.get("preserve_clipboard", False)),
                dry_run=_optional_bool(arguments, "dry_run"),
                clipboard_backend=_optional_choice(
                    arguments, "clipboard_backend", ("auto", "wl-copy", "xclip", "xsel")
                ),
                hotkey_backend=_optional_choice(
                    arguments, "hotkey_backend", ("auto", "ydotool", "xdotool")
                ),
                delay_ms=_optional_nonnegative_int(arguments, "delay_ms"),
                restore_delay_ms=_optional_nonnegative_int(arguments, "restore_delay_ms"),
                restore_policy=_optional_choice(
                    arguments, "restore_policy", ("strict", "best-effort", "off")
                ),
            )
        )

    def _hotkey(self, arguments: dict[str, Any]) -> dict[str, Any]:
        keys = arguments.get("keys")
        if not isinstance(keys, list) or not all(isinstance(key, str) for key in keys):
            raise ValueError("keys must be a list of strings")
        return _to_mcp_value(
            self._require_runtime().hotkey(
                keys,
                dry_run=_optional_bool(arguments, "dry_run"),
                backend=_optional_choice(arguments, "backend", HOTKEY_BACKEND_CHOICES),
                delay_ms=_optional_nonnegative_int(arguments, "delay_ms"),
                key_delay_ms=_optional_nonnegative_int(arguments, "key_delay_ms"),
                repeat=_optional_positive_int(arguments, "repeat"),
                interval_ms=_optional_nonnegative_int(arguments, "interval_ms"),
                release_before=_optional_bool(arguments, "release_before"),
                release_after=_optional_bool(arguments, "release_after"),
            )
        )

    def _find_element(self, arguments: dict[str, Any]) -> list[dict[str, Any]]:
        return _to_mcp_value(
            self._require_runtime().find_element(
                _required_str(arguments, "selector"),
                vision_fallback=bool(arguments.get("vision_fallback", False)),
                app=_optional_string(arguments, "app"),
                window_title=_optional_string(arguments, "window_title"),
                window_id=_optional_string(arguments, "window_id"),
                vision_region=_optional_rect(arguments, "vision_region"),
                vision_edge_threshold=_optional_int(arguments, "vision_edge_threshold"),
                vision_min_width=_optional_int(arguments, "vision_min_width"),
                vision_min_height=_optional_int(arguments, "vision_min_height"),
                vision_min_component_pixels=_optional_int(
                    arguments, "vision_min_component_pixels"
                ),
                vision_max_elements=_optional_int(arguments, "vision_max_elements"),
                vision_merge_distance=_optional_int(arguments, "vision_merge_distance"),
            )
        )

    def _list_windows(self, arguments: dict[str, Any]) -> list[dict[str, Any]] | dict[str, Any]:
        query = _window_query_kwargs_from_arguments(arguments)
        runtime = self._require_runtime()
        if query["diagnose"]:
            return _to_mcp_value(runtime.list_windows_result(**query))
        return _to_mcp_value(runtime.list_windows(**query))

    def _desktop_focus(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().desktop_focus(
                _required_str(arguments, "app"),
                use_gnome_overview=bool(arguments.get("use_gnome_overview", True)),
                launch_if_needed=bool(arguments.get("launch_if_needed", True)),
                wait_after_focus_ms=int(arguments.get("wait_after_focus_ms", 1_000)),
                overview_wait_ms=int(arguments.get("overview_wait_ms", 800)),
                window_title=_optional_string(arguments, "window_title"),
                window_id=_optional_string(arguments, "window_id"),
                verify=bool(arguments.get("verify", False)),
            )
        )

    def _desktop_locate(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().desktop_locate(
                _required_str(arguments, "app"),
                _required_str(arguments, "target"),
                image_path=_optional_string(arguments, "image_path"),
                prefer_accessibility=bool(arguments.get("prefer_accessibility", True)),
                window_title=_optional_string(arguments, "window_title"),
                window_id=_optional_string(arguments, "window_id"),
            )
        )

    def _desktop_click(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().desktop_click(
                _required_str(arguments, "app"),
                _required_str(arguments, "target"),
                image_path=_optional_string(arguments, "image_path"),
                prefer_accessibility=bool(arguments.get("prefer_accessibility", True)),
                window_title=_optional_string(arguments, "window_title"),
                window_id=_optional_string(arguments, "window_id"),
                button=_optional_string(arguments, "button") or "left",
                dry_run=bool(arguments.get("dry_run", False)),
                verify=bool(arguments.get("verify", False)),
            )
        )

    def _desktop_drag(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().desktop_drag(
                _required_str(arguments, "app"),
                _required_str(arguments, "target"),
                image_path=_optional_string(arguments, "image_path"),
                prefer_accessibility=bool(arguments.get("prefer_accessibility", True)),
                window_title=_optional_string(arguments, "window_title"),
                window_id=_optional_string(arguments, "window_id"),
                button=_optional_string(arguments, "button") or "left",
                from_ratio=_optional_ratio_pair(arguments, "from_ratio") or (0.5, 0.5),
                to_ratio=_optional_ratio_pair(arguments, "to_ratio") or (0.5, 0.5),
                duration_ms=int(arguments.get("duration_ms", 250)),
                dry_run=bool(arguments.get("dry_run", False)),
                verify=bool(arguments.get("verify", False)),
            )
        )

    def _desktop_type_into(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().desktop_type_into(
                _required_str(arguments, "app"),
                _required_str(arguments, "target"),
                _required_str(arguments, "text"),
                image_path=_optional_string(arguments, "image_path"),
                prefer_accessibility=bool(arguments.get("prefer_accessibility", True)),
                window_title=_optional_string(arguments, "window_title"),
                window_id=_optional_string(arguments, "window_id"),
                clear=bool(arguments.get("clear", False)),
                dry_run=bool(arguments.get("dry_run", False)),
                verify=bool(arguments.get("verify", False)),
            )
        )

    def _desktop_assert(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().desktop_assert(
                _required_str(arguments, "app"),
                _required_str(arguments, "target"),
                assertion=_optional_string(arguments, "assertion") or "present",
                expected_text=_optional_string(arguments, "expected_text"),
                image_path=_optional_string(arguments, "image_path"),
                prefer_accessibility=bool(arguments.get("prefer_accessibility", True)),
                window_title=_optional_string(arguments, "window_title"),
                window_id=_optional_string(arguments, "window_id"),
            )
        )

    def _list_plugins(self, arguments: dict[str, Any]) -> dict[str, Any]:
        paths = arguments.get("paths")
        if paths is not None and (
            not isinstance(paths, list) or not all(isinstance(path, str) for path in paths)
        ):
            raise ValueError("paths must be a list of plugin path strings")
        return _to_mcp_value(self._require_runtime().list_plugins(paths=paths))

    def _call_plugin_tool(self, arguments: dict[str, Any]) -> dict[str, Any]:
        paths = arguments.get("paths")
        if paths is not None and (
            not isinstance(paths, list) or not all(isinstance(path, str) for path in paths)
        ):
            raise ValueError("paths must be a list of plugin path strings")
        plugin_arguments = arguments.get("arguments") or {}
        if not isinstance(plugin_arguments, dict):
            raise ValueError("arguments must be an object")
        return _to_mcp_value(
            self._require_runtime().call_plugin_tool(
                plugin_id=_required_str(arguments, "plugin_id"),
                tool=_required_str(arguments, "tool"),
                arguments=plugin_arguments,
                paths=paths,
                timeout_seconds=float(arguments.get("timeout_seconds", 10.0)),
                max_output_bytes=int(arguments.get("max_output_bytes", 1_048_576)),
            )
        )

    def _get_desktop_state(self, _arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(self._require_runtime().get_desktop_state())

    def _ingest_desktop_snapshot(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().ingest_desktop_snapshot(
                snapshot_id=_optional_string(arguments, "snapshot_id"),
            )
        )

    def _latest_desktop_snapshot(self, _arguments: dict[str, Any]) -> dict[str, Any] | None:
        return _to_mcp_value(self._require_runtime().latest_desktop_snapshot())

    def _record_desktop_event(self, arguments: dict[str, Any]) -> dict[str, Any]:
        payload = arguments.get("payload")
        if payload is not None and not isinstance(payload, dict):
            raise ValueError("payload must be an object")
        return _to_mcp_value(
            self._require_runtime().record_desktop_event(
                kind=_required_str(arguments, "kind"),
                source=_optional_string(arguments, "source") or "mcp",
                target_id=_optional_string(arguments, "target_id"),
                payload=payload,
                occurred_at_unix_ms=_optional_int(arguments, "occurred_at_unix_ms"),
            )
        )

    def _desktop_graph_status(self, _arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(self._require_runtime().desktop_graph_status())

    def _refresh_desktop_graph(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().refresh_desktop_graph(
                snapshot_id=_optional_string(arguments, "snapshot_id"),
            )
        )

    def _query_desktop_graph(self, arguments: dict[str, Any]) -> list[dict[str, Any]]:
        attribute_equals = arguments.get("attribute_equals")
        if attribute_equals is not None and not isinstance(attribute_equals, dict):
            raise ValueError("attribute_equals must be an object")
        return _to_mcp_value(
            self._require_runtime().query_desktop_graph(
                kind=_optional_string(arguments, "kind"),
                label_contains=_optional_string(arguments, "label_contains"),
                role=_optional_string(arguments, "role"),
                attribute_equals=attribute_equals,
                contained_by=_optional_string(arguments, "contained_by"),
                latest_only=bool(arguments.get("latest_only", True)),
                refresh_if_stale=bool(arguments.get("refresh_if_stale", False)),
            )
        )

    def _execute_goal(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().execute_goal(
                _required_str(arguments, "goal"),
                replan_on_failure=bool(arguments.get("replan_on_failure", True)),
                max_replans=int(arguments.get("max_replans", 1)),
            )
        )

    def _generate_workflow(self, arguments: dict[str, Any]) -> dict[str, Any]:
        workflow = self._require_runtime().generate_workflow(
            _required_str(arguments, "goal"),
            refresh_desktop_graph=bool(arguments.get("refresh_desktop_graph", False)),
        )
        format_name = _optional_string(arguments, "format") or "json"
        return {
            "workflow": workflow_to_dict(workflow),
            "format": format_name,
            "text": dump_workflow_text(workflow, format_name=format_name),
        }

    def _save_generated_workflow(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return {
            "path": self._require_runtime().save_generated_workflow(
                _required_str(arguments, "goal"),
                _required_str(arguments, "path"),
                format_name=_optional_string(arguments, "format"),
                refresh_desktop_graph=bool(arguments.get("refresh_desktop_graph", False)),
            )
        }

    def _refine_workflow(self, arguments: dict[str, Any]) -> dict[str, Any]:
        workflow = self._require_runtime().refine_workflow(
            _required_str(arguments, "goal"),
            workflow=_optional_workflow(arguments, "workflow"),
            refresh_desktop_graph=bool(arguments.get("refresh_desktop_graph", False)),
        )
        format_name = _optional_string(arguments, "format") or "json"
        return {
            "workflow": workflow_to_dict(workflow),
            "format": format_name,
            "text": dump_workflow_text(workflow, format_name=format_name),
        }

    def _save_refined_workflow(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return {
            "path": self._require_runtime().save_refined_workflow(
                _required_str(arguments, "goal"),
                _required_str(arguments, "path"),
                workflow=_optional_workflow(arguments, "workflow"),
                format_name=_optional_string(arguments, "format"),
                refresh_desktop_graph=bool(arguments.get("refresh_desktop_graph", False)),
            )
        }

    def _execute_workflow(self, arguments: dict[str, Any]) -> dict[str, Any]:
        workflow = workflow_from_dict(arguments, default_name="mcp workflow")
        return _to_mcp_value(self._require_runtime().execute_workflow(workflow))

    def _execute_workflow_file(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().execute_workflow_file(
                _required_str(arguments, "path"),
            )
        )

    def _start_workflow_recording(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().start_recording(
                _optional_string(arguments, "name") or "recorded-workflow"
            )
        )

    def _stop_workflow_recording(self, _arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(self._require_runtime().stop_recording())

    def _get_recorded_workflow(self, _arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(self._require_runtime().recorded_workflow())

    def _save_recorded_workflow(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return {
            "path": self._require_runtime().save_recording(
                _required_str(arguments, "path"),
                format_name=_optional_string(arguments, "format"),
            )
        }

    def _ocr_screen(self, arguments: dict[str, Any]) -> dict[str, Any]:
        config = arguments.get("config") or []
        if not isinstance(config, list) or not all(isinstance(item, str) for item in config):
            raise ValueError("config must be a list of key=value strings")
        return _to_mcp_value(
            self._require_runtime().ocr_screen(
                region=_optional_rect(arguments, "region"),
                language=_optional_string(arguments, "language"),
                image_path=_optional_string(arguments, "image_path"),
                window_id=_optional_string(arguments, "window_id"),
                window_title=_optional_string(arguments, "window_title"),
                app=_optional_string(arguments, "app"),
                page_segmentation_mode=_optional_int(
                    arguments, "page_segmentation_mode"
                ),
                engine_mode=_optional_int(arguments, "engine_mode"),
                dpi=_optional_int(arguments, "dpi"),
                min_confidence=_optional_float(arguments, "min_confidence"),
                whitelist=_optional_string(arguments, "whitelist"),
                config=config,
                scale=_optional_float(arguments, "scale"),
                grayscale=_optional_bool(arguments, "grayscale"),
                threshold=_optional_int(arguments, "threshold"),
                invert=_optional_bool(arguments, "invert"),
                contrast=_optional_float(arguments, "contrast"),
                deskew=_optional_bool(arguments, "deskew"),
            )
        )

    def _compare_images(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().compare_image_files(
                _required_str(arguments, "expected_path"),
                _required_str(arguments, "actual_path"),
                region=_optional_rect(arguments, "region"),
                ignore_regions=_optional_rects(arguments, "ignore_regions"),
                per_channel_threshold=_optional_int(arguments, "per_channel_threshold"),
                max_changed_ratio=_optional_float(arguments, "max_changed_ratio"),
                max_changed_pixels=_optional_int(arguments, "max_changed_pixels"),
                max_mean_absolute_error=_optional_float(
                    arguments, "max_mean_absolute_error"
                ),
                max_channel_delta=_optional_int(arguments, "max_channel_delta"),
                size_policy=_optional_string(arguments, "size_policy"),
                alpha=_optional_string(arguments, "alpha"),
            )
        )

    def _detect_ui_state(self, arguments: dict[str, Any]) -> dict[str, Any]:
        image_paths = arguments.get("image_paths")
        if not isinstance(image_paths, list) or not all(
            isinstance(path, str) for path in image_paths
        ):
            raise ValueError("image_paths must be a list of image path strings")
        return _to_mcp_value(
            self._require_runtime().detect_ui_state_from_image_files(
                image_paths,
                region=_optional_rect(arguments, "region"),
                ignore_regions=_optional_rects(arguments, "ignore_regions"),
                per_channel_threshold=_optional_int(arguments, "per_channel_threshold"),
                stable_max_changed_ratio=_optional_float(
                    arguments, "stable_max_changed_ratio"
                ),
                stable_max_changed_pixels=_optional_int(
                    arguments, "stable_max_changed_pixels"
                ),
                stable_max_mean_absolute_error=_optional_float(
                    arguments, "stable_max_mean_absolute_error"
                ),
                stable_max_channel_delta=_optional_int(
                    arguments, "stable_max_channel_delta"
                ),
                loading_min_changed_ratio=_optional_float(
                    arguments, "loading_min_changed_ratio"
                ),
                loading_min_changed_pixels=_optional_int(
                    arguments, "loading_min_changed_pixels"
                ),
                required_stable_transitions=_optional_int(
                    arguments, "required_stable_transitions"
                ),
                size_policy=_optional_string(arguments, "size_policy"),
                alpha=_optional_string(arguments, "alpha"),
            )
        )

    def _detect_ui_elements(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().detect_ui_elements_from_image_file(
                _required_str(arguments, "image_path"),
                region=_optional_rect(arguments, "region"),
                ignore_regions=_optional_rects(arguments, "ignore_regions"),
                edge_threshold=_optional_int(arguments, "edge_threshold"),
                min_width=_optional_int(arguments, "min_width"),
                min_height=_optional_int(arguments, "min_height"),
                min_component_pixels=_optional_int(arguments, "min_component_pixels"),
                min_confidence=_optional_float(arguments, "min_confidence"),
                max_width=_optional_int(arguments, "max_width"),
                max_height=_optional_int(arguments, "max_height"),
                min_area=_optional_int(arguments, "min_area"),
                max_area=_optional_int(arguments, "max_area"),
                max_elements=_optional_int(arguments, "max_elements"),
                merge_distance=_optional_int(arguments, "merge_distance"),
                padding=_optional_int(arguments, "padding"),
                sort=_optional_string(arguments, "sort"),
                mask_output_path=_optional_string(arguments, "mask_output_path"),
                overlay_output_path=_optional_string(arguments, "overlay_output_path"),
            )
        )

    def _find_elements(self, arguments: dict[str, Any]) -> list[dict[str, Any]]:
        elements = self._find_element(arguments)
        limit = _optional_positive_int(arguments, "limit")
        if limit is not None:
            elements = elements[:limit]
        return elements

    def _desktop_profiles(self, arguments: dict[str, Any]) -> dict[str, Any]:
        if self.runtime is None:
            return {
                "schema_version": "desktop-profiles.v1",
                "count": 0,
                "profiles": [],
                "runtime_available": False,
            }
        result = self.runtime.desktop_profiles(
            _optional_string(arguments, "app"),
            target=_optional_string(arguments, "target"),
            command=_optional_string(arguments, "command"),
            desktop_id=_optional_string(arguments, "desktop_id"),
            supports=_optional_string(arguments, "supports"),
            check=_optional_bool(arguments, "check"),
            installed=_optional_bool(arguments, "installed"),
            available=_optional_bool(arguments, "available"),
        )
        payload = _to_mcp_value(result)
        if isinstance(payload, dict):
            return payload
        return {"schema_version": "desktop-profiles.v1", "count": 0, "profiles": []}

    def _desktop_profile_completion_values(self) -> tuple[str, ...]:
        try:
            payload = self._desktop_profiles({})
        except Exception:
            return ()
        values: list[str] = []
        for profile in payload.get("profiles", []):
            if not isinstance(profile, dict):
                continue
            profile_id = profile.get("id")
            if isinstance(profile_id, str):
                values.append(profile_id)
            aliases = profile.get("aliases")
            if isinstance(aliases, list):
                values.extend(alias for alias in aliases if isinstance(alias, str))
        return tuple(dict.fromkeys(values))

    def _desktop_target_completion_values(self, app: str | None = None) -> tuple[str, ...]:
        try:
            payload = self._desktop_profiles({"app": app} if app else {})
        except Exception:
            return ()
        values: list[str] = []
        for profile in payload.get("profiles", []):
            if not isinstance(profile, dict):
                continue
            targets = profile.get("targets")
            if not isinstance(targets, list):
                continue
            for target in targets:
                if isinstance(target, dict) and isinstance(target.get("name"), str):
                    values.append(target["name"])
                elif isinstance(target, str):
                    values.append(target)
        return tuple(dict.fromkeys(values))

    def _plan(self, arguments: dict[str, Any]) -> dict[str, Any]:
        steps = self._require_runtime().plan(_required_str(arguments, "goal"))
        return {"steps": steps}

    def _plan_workflow(self, arguments: dict[str, Any]) -> dict[str, Any]:
        workflow = self._require_runtime().plan_workflow(_required_str(arguments, "goal"))
        format_name = _optional_string(arguments, "format") or "json"
        return {
            "workflow": workflow_to_dict(workflow),
            "format": format_name,
            "text": dump_workflow_text(workflow, format_name=format_name),
        }

    def _replan_workflow(self, arguments: dict[str, Any]) -> dict[str, Any]:
        failed_workflow = workflow_from_dict(
            _required_object(arguments, "failed_workflow"),
            default_name="failed workflow",
        )
        failed_result = _workflow_execution_result_from_mcp(
            _required_str(arguments, "goal"),
            arguments.get("failed_result"),
        )
        workflow = self._require_runtime().replan_workflow(
            _required_str(arguments, "goal"),
            failed_workflow,
            failed_result,
            refresh_desktop_graph=bool(arguments.get("refresh_desktop_graph", False)),
        )
        format_name = _optional_string(arguments, "format") or "json"
        return {
            "workflow": workflow_to_dict(workflow),
            "format": format_name,
            "text": dump_workflow_text(workflow, format_name=format_name),
        }

    def _load_workflow_file(self, arguments: dict[str, Any]) -> dict[str, Any]:
        workflow = self._require_runtime().load_workflow_file(_required_str(arguments, "path"))
        return {"workflow": workflow_to_dict(workflow)}

    def _query_desktop_edges(self, arguments: dict[str, Any]) -> list[dict[str, Any]]:
        return _to_mcp_value(
            self._require_runtime().query_desktop_edges(
                source=_optional_string(arguments, "source"),
                target=_optional_string(arguments, "target"),
                kind=_optional_string(arguments, "kind"),
                latest_only=bool(arguments.get("latest_only", True)),
            )
        )

    def _capability_audit(self, _arguments: dict[str, Any]) -> dict[str, Any]:
        return {"events": _to_mcp_value(self._require_runtime().capability_audit())}

    def _confirmation_audit(self, _arguments: dict[str, Any]) -> dict[str, Any]:
        return {"events": _to_mcp_value(self._require_runtime().confirmation_audit())}

    def _preflight_audit(self, _arguments: dict[str, Any]) -> dict[str, Any]:
        return {"events": _to_mcp_value(self._require_runtime().preflight_audit())}


def _schema(
    properties: dict[str, Any],
    required: list[str] | None = None,
    any_of: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    schema: dict[str, Any] = {
        "type": "object",
        "properties": properties,
        "additionalProperties": False,
    }
    if required:
        schema["required"] = required
    if any_of:
        schema["anyOf"] = any_of
    return schema


def _ocr_input_schema() -> dict[str, Any]:
    return _schema(
        {
            "region": RECT_SCHEMA,
            "language": {"type": "string"},
            "image_path": {"type": "string"},
            "window_id": {"type": "string"},
            "window_title": {"type": "string"},
            "app": {"type": "string"},
            "page_segmentation_mode": {
                "type": "integer",
                "minimum": 0,
                "maximum": 13,
            },
            "engine_mode": {"type": "integer", "minimum": 0, "maximum": 3},
            "dpi": {"type": "integer", "minimum": 1},
            "min_confidence": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
            },
            "whitelist": {"type": "string"},
            "config": {
                "type": "array",
                "items": {"type": "string"},
            },
            "scale": {"type": "number", "minimum": 0.1, "maximum": 8.0},
            "grayscale": {"type": "boolean", "default": False},
            "threshold": {"type": "integer", "minimum": 0, "maximum": 255},
            "invert": {"type": "boolean", "default": False},
            "contrast": {
                "type": "number",
                "minimum": -255.0,
                "maximum": 255.0,
            },
            "deskew": {"type": "boolean", "default": False},
        }
    )


def _optional_rect(arguments: dict[str, Any], name: str) -> Rect | None:
    value = arguments.get(name)
    if value is None:
        return None
    if isinstance(value, Rect):
        return value
    if not isinstance(value, dict):
        raise ValueError(f"{name} must be a rectangle object")
    return Rect(
        x=int(value["x"]),
        y=int(value["y"]),
        width=int(value["width"]),
        height=int(value["height"]),
    )


def _optional_rects(arguments: dict[str, Any], name: str) -> tuple[Rect, ...] | None:
    value = arguments.get(name)
    if value is None:
        return None
    if not isinstance(value, list):
        raise ValueError(f"{name} must be a rectangle array")
    rects = []
    for item in value:
        rect = _optional_rect({name: item}, name)
        if rect is None:
            raise ValueError(f"{name} must not contain null rectangles")
        rects.append(rect)
    return tuple(rects)


def _required_str(arguments: dict[str, Any], name: str) -> str:
    value = arguments.get(name)
    if not isinstance(value, str) or not value:
        raise ValueError(f"{name} must be a non-empty string")
    return value


def _required_object(arguments: dict[str, Any], name: str) -> dict[str, Any]:
    value = arguments.get(name)
    if not isinstance(value, dict):
        raise ValueError(f"{name} must be an object")
    return value


def _required_string_array(arguments: dict[str, Any], name: str) -> tuple[str, ...]:
    value = arguments.get(name)
    if not isinstance(value, list) or not value:
        raise ValueError(f"{name} must be a non-empty string array")
    items = tuple(item for item in value if isinstance(item, str) and item)
    if len(items) != len(value):
        raise ValueError(f"{name} must contain only non-empty strings")
    return items


def _optional_int(arguments: dict[str, Any], name: str) -> int | None:
    value = arguments.get(name)
    return None if value is None else int(value)


def _optional_float(arguments: dict[str, Any], name: str) -> float | None:
    value = arguments.get(name)
    return None if value is None else float(value)


def _positive_float(value: str) -> float:
    parsed = float(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return parsed


def _env_positive_float(name: str, default: float) -> float:
    value = os.environ.get(name)
    if value is None:
        return default
    parsed = float(value)
    if parsed <= 0:
        raise ValueError(f"{name} must be greater than zero")
    return parsed


def _optional_bool(arguments: dict[str, Any], name: str) -> bool:
    value = arguments.get(name)
    if value is None:
        return False
    if not isinstance(value, bool):
        raise ValueError(f"{name} must be a boolean")
    return value


def _optional_positive_int(arguments: dict[str, Any], name: str) -> int | None:
    value = _optional_int(arguments, name)
    if value is not None and value <= 0:
        raise ValueError(f"{name} must be greater than zero")
    return value


def _optional_nonnegative_int(arguments: dict[str, Any], name: str) -> int | None:
    value = _optional_int(arguments, name)
    if value is not None and value < 0:
        raise ValueError(f"{name} must be non-negative")
    return value


def _optional_choice(
    arguments: dict[str, Any],
    name: str,
    choices: tuple[str, ...],
) -> str | None:
    value = _optional_string(arguments, name)
    if value is None:
        return None
    if value not in choices:
        expected = ", ".join(choices)
        raise ValueError(f"{name} must be one of: {expected}")
    return value


def _window_query_kwargs_from_arguments(arguments: dict[str, Any]) -> dict[str, Any]:
    return {
        "id": _optional_string(arguments, "id"),
        "app": _optional_string(arguments, "app"),
        "title": _optional_string(arguments, "title"),
        "title_regex": _optional_string(arguments, "title_regex"),
        "focused": _optional_bool(arguments, "focused"),
        "limit": _optional_positive_int(arguments, "limit"),
        "sort": _optional_choice(arguments, "sort", WINDOW_SORT_CHOICES),
        "backend": _optional_choice(arguments, "backend", WINDOW_BACKEND_CHOICES),
        "diagnose": _optional_bool(arguments, "diagnose"),
    }


def _optional_ratio_pair(arguments: dict[str, Any], name: str) -> tuple[float, float] | None:
    value = arguments.get(name)
    if value is None:
        return None
    if not isinstance(value, (list, tuple)) or len(value) != 2:
        raise ValueError(f"{name} must be a two-item ratio array")
    ratio = (float(value[0]), float(value[1]))
    if not all(0.0 <= part <= 1.0 for part in ratio):
        raise ValueError(f"{name} values must be between 0.0 and 1.0")
    return ratio


def _optional_string(arguments: dict[str, Any], name: str) -> str | None:
    value = arguments.get(name)
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(f"{name} must be a string")
    return value


def _optional_workflow(arguments: dict[str, Any], name: str) -> Any:
    value = arguments.get(name)
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ValueError(f"{name} must be a workflow object")
    return workflow_from_dict(value, default_name=_required_str(arguments, "goal"))


def _to_mcp_value(value: Any) -> Any:
    if is_dataclass(value):
        return {field.name: _to_mcp_value(getattr(value, field.name)) for field in fields(value)}
    if isinstance(value, bytes):
        return base64.b64encode(value).decode("ascii")
    if isinstance(value, tuple | list):
        return [_to_mcp_value(item) for item in value]
    if isinstance(value, dict):
        return {str(key): _to_mcp_value(item) for key, item in value.items()}
    if isinstance(value, Path):
        return str(value)
    return value


def _preflight_error_content(error: PreflightError, tool: str) -> dict[str, Any]:
    preflight = _to_mcp_value(error.result)
    return {
        "error": type(error).__name__,
        "message": str(error),
        "tool": tool,
        "next_action": "run_doctor",
        "blocked_categories": preflight["blocked_categories"],
        "warning_categories": preflight["warning_categories"],
        "required_categories": preflight["required_categories"],
        "category_status": preflight["category_status"],
        "category_severity": preflight["category_severity"],
        "preflight": preflight,
    }


def _capability_error_content(error: CapabilityDeniedError, tool: str) -> dict[str, Any]:
    return {
        "error": type(error).__name__,
        "message": str(error),
        "tool": tool,
        "capability": error.capability,
        "operation": error.operation,
        "retryable": False,
        "category": "capability",
        "next_action": "adjust_capability_profile",
        "suggested_tools": ("capability_audit",),
    }


def _confirmation_error_content(
    error: ConfirmationRequiredError | ConfirmationDeniedError,
    tool: str,
    *,
    required: bool,
) -> dict[str, Any]:
    return {
        "error": type(error).__name__,
        "message": str(error),
        "tool": tool,
        "action": error.action,
        "operation": error.operation,
        "retryable": required,
        "category": "confirmation",
        "next_action": "request_confirmation" if required else "stop",
        "suggested_tools": ("confirmation_audit",),
    }


def _generic_error_content(error: Exception, tool: str) -> dict[str, Any]:
    return {
        "error": type(error).__name__,
        "message": str(error),
        "tool": tool,
        "retryable": False,
        "category": "runtime",
        "next_action": "inspect_error",
    }


def _tool_result_content(structured: Any) -> list[dict[str, Any]]:
    content: list[dict[str, Any]] = []
    if isinstance(structured, dict):
        image_base64 = structured.get("image_base64")
        mime_type = structured.get("mime_type")
        if isinstance(image_base64, str) and isinstance(mime_type, str):
            if mime_type.startswith("image/"):
                content.append(
                    {
                        "type": "image",
                        "data": image_base64,
                        "mimeType": mime_type,
                    }
                )
    content.append(
        {
            "type": "text",
            "text": json.dumps(structured, ensure_ascii=False, sort_keys=True),
        }
    )
    return content


def _json_resource(uri: str, value: Any) -> dict[str, Any]:
    return {
        "uri": uri,
        "mimeType": "application/json",
        "text": json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True),
    }


def _text_resource(uri: str, text: str, *, mime_type: str) -> dict[str, Any]:
    return {"uri": uri, "mimeType": mime_type, "text": text}


def _repo_root() -> Path:
    for parent in Path(__file__).resolve().parents:
        if (parent / "Cargo.toml").is_file() and (parent / "python").is_dir():
            return parent
    return Path.cwd()


def _read_repo_text(path: str) -> str:
    root = _repo_root()
    target = (root / path).resolve()
    try:
        target.relative_to(root.resolve())
    except ValueError as error:
        raise JsonRpcProtocolError(INVALID_PARAMS, f"resource path escapes repository: {path}") from error
    if not target.is_file():
        raise JsonRpcProtocolError(INVALID_PARAMS, f"resource file not found: {path}")
    return target.read_text(encoding="utf-8")


def _completion_context_value(params: dict[str, Any], name: str) -> str | None:
    for key in ("context", "arguments"):
        value = params.get(key)
        if isinstance(value, dict) and isinstance(value.get(name), str):
            return value[name]
    ref = params.get("ref")
    if isinstance(ref, dict):
        arguments = ref.get("arguments")
        if isinstance(arguments, dict) and isinstance(arguments.get(name), str):
            return arguments[name]
    return None


def _completion_matches(values: tuple[str, ...], value: str) -> list[str]:
    needle = value.casefold()
    if not needle:
        return list(values)
    prefix = [item for item in values if item.casefold().startswith(needle)]
    contains = [
        item
        for item in values
        if needle in item.casefold() and item not in prefix
    ]
    return prefix + contains


def _render_prompt(prompt: McpPrompt, arguments: dict[str, Any]) -> str:
    missing = [
        argument.name
        for argument in prompt.arguments
        if argument.required and not arguments.get(argument.name)
    ]
    if missing:
        raise ValueError(f"prompt {prompt.name} missing required argument(s): {', '.join(missing)}")
    rendered_args = {
        argument.name: arguments.get(argument.name)
        for argument in prompt.arguments
        if arguments.get(argument.name) is not None
    }
    header = f"Use PeekabooX MCP prompt `{prompt.name}`."
    body = {
        "diagnose-desktop": (
            "Run `doctor` first, then use `preflight` for the required categories. "
            "Read `peekaboox://doctor/latest` and explain blocked checks with concrete next actions."
        ),
        "safe-desktop-action": (
            "Plan the desktop action without hard-coded coordinates. Prefer desktop helper tools, "
            "scope by app/window, run strict preflight for required categories, and request "
            "confirmation before mutating input."
        ),
        "inspect-window": (
            "Use `list_windows`, `capture_screen` with `window_id`, `elements`, and OCR as needed. "
            "Summarize the target window, visible controls, and safest next tool call."
        ),
        "build-workflow": (
            "Generate an editable workflow with semantic selectors where possible. Do not execute it "
            "until the user confirms; include recovery expectations for failed selector replay."
        ),
        "recover-from-tool-error": (
            "Inspect structuredContent.error, next_action, capability/action fields, and preflight "
            "categories. Recommend the deterministic recovery path without parsing prose."
        ),
        "plugin-development": (
            "Use the PeekabooX plugin SDK manifest format, declare bounded process tools, and "
            "validate execution through list_plugins and call_plugin_tool."
        ),
        "ocr-visible-text": (
            "Use window-scoped capture or image OCR where possible. Include language, PSM, confidence, "
            "and preprocessing choices when text is small or noisy."
        ),
        "semantic-click-plan": (
            "Resolve semantic targets through the graph cache or elements first, use vision fallback "
            "only when accessibility misses, then perform a dry run or verified desktop_click."
        ),
    }[prompt.name]
    if rendered_args:
        return f"{header}\n\nArguments:\n{json.dumps(rendered_args, ensure_ascii=False, indent=2, sort_keys=True)}\n\n{body}"
    return f"{header}\n\n{body}"


def _workflow_execution_result_from_mcp(
    goal: str,
    value: Any,
) -> WorkflowExecutionResult:
    if isinstance(value, WorkflowExecutionResult):
        return value
    if not isinstance(value, dict):
        raise ValueError("failed_result must be an object")
    recovery = value.get("recovery")
    if recovery is None:
        recovery = {
            "failed_step": value.get("failed_step", 0),
            "reason": value.get("reason", "workflow failed"),
            "attempts": value.get("attempts", 0),
        }
    if not isinstance(recovery, dict):
        raise ValueError("failed_result.recovery must be an object")
    return WorkflowExecutionResult(
        goal=goal,
        ok=bool(value.get("ok", False)),
        steps=(),
        recovery=recovery,
    )


def _tool_title(name: str) -> str:
    return name.replace("_", " ").title()


def _tool_annotations(name: str) -> dict[str, Any]:
    mutating = {
        "call_plugin_tool",
        "capture_backends",
        "click",
        "move_mouse",
        "drag",
        "type_text",
        "paste_text",
        "hotkey",
        "desktop_focus",
        "desktop_click",
        "desktop_drag",
        "desktop_type_into",
        "execute_goal",
        "execute_workflow",
        "execute_workflow_file",
    }
    stateful = {
        "ingest_desktop_snapshot",
        "record_desktop_event",
        "refresh_desktop_graph",
        "start_workflow_recording",
        "stop_workflow_recording",
        "save_generated_workflow",
        "save_refined_workflow",
        "save_recorded_workflow",
    }
    generating = {
        "generate_workflow",
        "refine_workflow",
        "replan_workflow",
    }
    read_only = name not in mutating and name not in stateful and name not in generating
    return {
        "readOnlyHint": read_only,
        "destructiveHint": name in mutating,
        "idempotentHint": read_only,
        "openWorldHint": name not in {"compare_images", "detect_ui_state", "detect_ui_elements", "vision_elements"},
    }


def _tool_output_schema(name: str) -> dict[str, Any]:
    object_schema = {"type": "object", "additionalProperties": True}
    array_schema = {"type": "array", "items": {"type": "object", "additionalProperties": True}}
    desktop_action_schema = {
        "type": "object",
        "properties": {
            "app": {"type": "string"},
            "action": {"type": "string"},
            "detail": {"type": "string"},
            "backend_name": {"type": "string"},
            "verified": {"type": "boolean"},
            "verification_detail": {"anyOf": [{"type": "string"}, {"type": "null"}]},
            "focus_diagnostics": {
                "type": "array",
                "items": {"type": "string"},
            },
        },
        "required": ["app", "action", "detail", "backend_name", "verified", "focus_diagnostics"],
        "additionalProperties": True,
    }
    if name == "capture_screen":
        return {
            "type": "object",
            "properties": {
                "image_base64": {"type": "string"},
                "mime_type": {"type": "string"},
                "semantic_tree": array_schema,
                "metadata": object_schema,
            },
            "required": ["image_base64", "mime_type", "metadata"],
            "additionalProperties": True,
        }
    if name in {"find_element", "find_elements", "elements", "list_windows", "query_desktop_graph", "query_desktop_edges"}:
        return {"oneOf": [array_schema, object_schema]}
    if name in {"click", "move_mouse", "drag", "type_text", "paste_text", "hotkey"}:
        return {
            "type": "object",
            "properties": {
                "ok": {"type": "boolean"},
                "message": {"type": "string"},
            },
            "required": ["ok", "message"],
            "additionalProperties": True,
        }
    if name in {
        "desktop_focus",
        "desktop_click",
        "desktop_drag",
        "desktop_type_into",
        "desktop_assert",
    }:
        return desktop_action_schema
    return object_schema


def _jsonrpc_error(request_id: Any, code: int, message: str) -> dict[str, Any]:
    return {
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {"code": code, "message": message},
    }


def create_server(
    target: str | None = None,
    connect: bool = True,
    capability_policy: CapabilityPolicy | None = None,
    capability_profile: str | None = None,
    confirmation_policy: ConfirmationPolicy | None = None,
    audit_log_path: str | os.PathLike[str] | None = None,
    plugin_paths: tuple[str | os.PathLike[str], ...] = (),
    preflight_mode: str | None = None,
    preflight_timeout_seconds: float = 30.0,
    client_timeout_seconds: float | None = None,
) -> McpServer:
    runtime = None
    if connect:
        runtime = AgentRuntime.connect(
            target or os.environ.get("PEEKABOOX_GRPC_TARGET", "127.0.0.1:47777"),
            capability_policy=capability_policy,
            capability_profile=capability_profile,
            confirmation_policy=confirmation_policy,
            audit_log_path=audit_log_path,
            audit_source="mcp",
            plugin_paths=plugin_paths,
            preflight_mode=preflight_mode,
            preflight_timeout_seconds=preflight_timeout_seconds,
            client_timeout_seconds=client_timeout_seconds,
        )
    else:
        audit_logger = (
            JsonlAuditLogger(audit_log_path, source="mcp")
            if audit_log_path is not None
            else None
        )
        runtime = AgentRuntime(
            capability_policy=capability_policy
            or (
                CapabilityPolicy.from_profile(capability_profile, audit_logger=audit_logger)
                if capability_profile is not None
                else CapabilityPolicy.from_env(audit_logger=audit_logger)
            ),
            confirmation_policy=confirmation_policy or ConfirmationPolicy.disabled(),
            audit_logger=audit_logger,
            plugin_paths=tuple(Path(path) for path in plugin_paths),
            preflight_mode=preflight_mode,
            preflight_timeout_seconds=preflight_timeout_seconds,
        )
    server = McpServer(runtime=runtime)
    server.register_default_tools()
    return server


def main() -> None:
    parser = argparse.ArgumentParser(description="PeekabooX MCP server")
    parser.add_argument(
        "--target",
        default=os.environ.get("PEEKABOOX_GRPC_TARGET", "127.0.0.1:47777"),
        help="PeekabooX daemon gRPC target",
    )
    parser.add_argument(
        "--list-tools",
        action="store_true",
        help="print registered MCP tools and exit",
    )
    parser.add_argument(
        "--transport",
        choices=("stdio", "http", "sse"),
        default=os.environ.get("PEEKABOOX_MCP_TRANSPORT", "stdio"),
        help="MCP transport to serve",
    )
    parser.add_argument(
        "--host",
        default=os.environ.get("PEEKABOOX_MCP_HOST", "127.0.0.1"),
        help="HTTP/SSE bind host",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=int(os.environ.get("PEEKABOOX_MCP_PORT", "47778")),
        help="HTTP/SSE bind port",
    )
    parser.add_argument(
        "--audit-log",
        default=os.environ.get("PEEKABOOX_RUNTIME_AUDIT_LOG"),
        help="write runtime/MCP capability and confirmation checks as JSONL",
    )
    parser.add_argument(
        "--capability-profile",
        default=os.environ.get("PEEKABOOX_MCP_CAPABILITY_PROFILE")
        or os.environ.get("PEEKABOOX_CAPABILITY_PROFILE"),
        help=(
            "runtime capability profile for MCP tool calls; known profiles: "
            + ", ".join(KNOWN_CAPABILITY_PROFILES)
        ),
    )
    parser.add_argument(
        "--preflight-mode",
        choices=("off", "warn", "strict"),
        default=os.environ.get("PEEKABOOX_PREFLIGHT_MODE"),
        help="Doctor-backed preflight mode for live MCP tool calls",
    )
    parser.add_argument(
        "--preflight-timeout",
        type=_positive_float,
        default=_env_positive_float("PEEKABOOX_PREFLIGHT_TIMEOUT", 30.0),
        help="maximum seconds to wait for preflight Doctor checks",
    )
    parser.add_argument(
        "--grpc-timeout",
        type=_positive_float,
        default=_env_positive_float("PEEKABOOX_GRPC_TIMEOUT", DEFAULT_GRPC_TIMEOUT_SECONDS),
        help="maximum seconds to wait for each daemon gRPC call",
    )
    parser.add_argument(
        "--plugin-path",
        action="append",
        default=[],
        help="additional plugin directory or manifest path; repeatable",
    )
    args = parser.parse_args()

    try:
        server = create_server(
            args.target,
            connect=True,
            capability_profile=args.capability_profile,
            audit_log_path=args.audit_log,
            plugin_paths=tuple(args.plugin_path),
            preflight_mode=args.preflight_mode,
            preflight_timeout_seconds=args.preflight_timeout,
            client_timeout_seconds=args.grpc_timeout,
        )
    except ImportError:
        server = create_server(
            args.target,
            connect=False,
            capability_profile=args.capability_profile,
            audit_log_path=args.audit_log,
            plugin_paths=tuple(args.plugin_path),
            preflight_mode=args.preflight_mode,
            preflight_timeout_seconds=args.preflight_timeout,
            client_timeout_seconds=args.grpc_timeout,
        )
    if args.list_tools:
        print("peekaboox-mcp tools:", ", ".join(tool["name"] for tool in server.list_tools()))
        return
    if args.transport == "stdio":
        server.serve_stdio()
        return
    server.serve_http(args.host, args.port, sse=args.transport == "sse")


if __name__ == "__main__":
    main()
