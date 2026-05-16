from __future__ import annotations

import argparse
import base64
import json
import os
import sys
from collections.abc import Callable
from dataclasses import dataclass, field, fields, is_dataclass
from pathlib import Path
from typing import Any, TextIO

from peekaboox.agent import AgentRuntime
from peekaboox.agent.runtime import WINDOW_BACKEND_CHOICES, WINDOW_SORT_CHOICES
from peekaboox.client import Rect
from peekaboox.security import (
    KNOWN_CAPABILITY_PROFILES,
    CapabilityPolicy,
    ConfirmationPolicy,
    JsonlAuditLogger,
)
from peekaboox.workflows import dump_workflow_text, workflow_from_dict, workflow_to_dict


MCP_PROTOCOL_VERSION = "2025-11-25"
SERVER_NAME = "peekaboox-mcp"
SERVER_VERSION = "1.0.1"

PARSE_ERROR = -32700
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INVALID_PARAMS = -32602
INTERNAL_ERROR = -32603

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
        "button": {"type": "string", "enum": ["left", "middle", "right"]},
        "duration_ms": {"type": "integer", "minimum": 0},
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

    def descriptor(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        }

    def call(self, arguments: dict[str, Any] | None = None) -> Any:
        if self.handler is None:
            raise RuntimeError(f"MCP tool {self.name!r} is not bound to an AgentRuntime")
        return self.handler(arguments or {})

    def __call__(self, **arguments: Any) -> Any:
        return self.call(arguments)


@dataclass(slots=True)
class McpServer:
    """Registry and dispatcher for PeekabooX MCP tool handlers."""

    runtime: AgentRuntime | None = None
    tools: dict[str, McpTool] = field(default_factory=dict)

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
                "capabilities": {"tools": {"listChanged": False}},
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
        except Exception as error:
            structured = {
                "error": type(error).__name__,
                "message": str(error),
                "tool": name,
            }
            is_error = True

        text = json.dumps(structured, ensure_ascii=False, sort_keys=True)
        return {
            "content": [{"type": "text", "text": text}],
            "structuredContent": structured,
            "isError": is_error,
        }

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
                        "vision_fallback": {"type": "boolean", "default": False},
                    },
                    any_of=[
                        {"required": ["x", "y"]},
                        {"required": ["selector"]},
                        {"required": ["semantic_selector"]},
                    ],
                ),
                self._click,
            ),
            self._tool(
                "move_mouse",
                "Move the pointer to screen coordinates through the daemon input backend.",
                _schema(
                    {
                        "x": {"type": "integer"},
                        "y": {"type": "integer"},
                    },
                    required=["x", "y"],
                ),
                self._move_mouse,
            ),
            self._tool(
                "drag",
                "Drag from one screen coordinate to another through the daemon input backend.",
                _schema(
                    {
                        "from_x": {"type": "integer"},
                        "from_y": {"type": "integer"},
                        "to_x": {"type": "integer"},
                        "to_y": {"type": "integer"},
                        "button": {"type": "string", "enum": ["left", "middle", "right"]},
                        "duration_ms": {"type": "integer", "minimum": 0, "default": 250},
                    },
                    required=["from_x", "from_y", "to_x", "to_y"],
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
                        "per_channel_threshold": {"type": "integer", "minimum": 0},
                        "max_changed_ratio": {"type": "number", "minimum": 0, "maximum": 1},
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
                        "per_channel_threshold": {"type": "integer", "minimum": 0},
                        "stable_max_changed_ratio": {"type": "number", "minimum": 0, "maximum": 1},
                        "loading_min_changed_ratio": {"type": "number", "minimum": 0, "maximum": 1},
                        "required_stable_transitions": {"type": "integer", "minimum": 1},
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
                        "edge_threshold": {"type": "integer", "minimum": 1},
                        "min_width": {"type": "integer", "minimum": 1},
                        "min_height": {"type": "integer", "minimum": 1},
                        "min_component_pixels": {"type": "integer", "minimum": 1},
                        "max_elements": {"type": "integer", "minimum": 1},
                        "merge_distance": {"type": "integer", "minimum": 0},
                    },
                    required=["image_path"],
                ),
                self._detect_ui_elements,
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

        if selector is not None:
            if has_coordinates:
                raise ValueError("provide either coordinates or selector, not both")
            return _to_mcp_value(
                runtime.click_selector(str(selector), vision_fallback=vision_fallback)
            )

        if "x" not in arguments or "y" not in arguments:
            raise ValueError("click requires x/y coordinates or selector")
        return _to_mcp_value(
            runtime.click(
                x=int(arguments["x"]),
                y=int(arguments["y"]),
                vision_fallback=vision_fallback,
            )
        )

    def _move_mouse(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().move_mouse(
                int(arguments["x"]),
                int(arguments["y"]),
            )
        )

    def _drag(self, arguments: dict[str, Any]) -> dict[str, Any]:
        duration_ms = int(arguments.get("duration_ms", 250))
        if duration_ms < 0:
            raise ValueError("duration_ms must be non-negative")
        return _to_mcp_value(
            self._require_runtime().drag(
                int(arguments["from_x"]),
                int(arguments["from_y"]),
                int(arguments["to_x"]),
                int(arguments["to_y"]),
                button=_optional_string(arguments, "button") or "left",
                duration_ms=duration_ms,
            )
        )

    def _type_text(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().type_text(
                _required_str(arguments, "text"),
                typing_speed_chars_per_second=_optional_int(
                    arguments, "typing_speed_chars_per_second"
                ),
            )
        )

    def _paste_text(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().paste_text(
                _required_str(arguments, "text"),
                preserve_clipboard=bool(arguments.get("preserve_clipboard", False)),
            )
        )

    def _hotkey(self, arguments: dict[str, Any]) -> dict[str, Any]:
        keys = arguments.get("keys")
        if not isinstance(keys, list) or not all(isinstance(key, str) for key in keys):
            raise ValueError("keys must be a list of strings")
        return _to_mcp_value(self._require_runtime().hotkey(keys))

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
                per_channel_threshold=_optional_int(arguments, "per_channel_threshold"),
                max_changed_ratio=_optional_float(arguments, "max_changed_ratio"),
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
                per_channel_threshold=_optional_int(arguments, "per_channel_threshold"),
                stable_max_changed_ratio=_optional_float(
                    arguments, "stable_max_changed_ratio"
                ),
                loading_min_changed_ratio=_optional_float(
                    arguments, "loading_min_changed_ratio"
                ),
                required_stable_transitions=_optional_int(
                    arguments, "required_stable_transitions"
                ),
            )
        )

    def _detect_ui_elements(self, arguments: dict[str, Any]) -> dict[str, Any]:
        return _to_mcp_value(
            self._require_runtime().detect_ui_elements_from_image_file(
                _required_str(arguments, "image_path"),
                region=_optional_rect(arguments, "region"),
                edge_threshold=_optional_int(arguments, "edge_threshold"),
                min_width=_optional_int(arguments, "min_width"),
                min_height=_optional_int(arguments, "min_height"),
                min_component_pixels=_optional_int(arguments, "min_component_pixels"),
                max_elements=_optional_int(arguments, "max_elements"),
                merge_distance=_optional_int(arguments, "merge_distance"),
            )
        )


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


def _required_str(arguments: dict[str, Any], name: str) -> str:
    value = arguments.get(name)
    if not isinstance(value, str) or not value:
        raise ValueError(f"{name} must be a non-empty string")
    return value


def _optional_int(arguments: dict[str, Any], name: str) -> int | None:
    value = arguments.get(name)
    return None if value is None else int(value)


def _optional_float(arguments: dict[str, Any], name: str) -> float | None:
    value = arguments.get(name)
    return None if value is None else float(value)


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
        )
    server = McpServer(runtime=runtime)
    server.register_default_tools()
    return server


def main() -> None:
    parser = argparse.ArgumentParser(description="PeekabooX MCP stdio server")
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
        )
    except ImportError:
        server = create_server(
            args.target,
            connect=False,
            capability_profile=args.capability_profile,
            audit_log_path=args.audit_log,
            plugin_paths=tuple(args.plugin_path),
        )
    if args.list_tools:
        print("peekaboox-mcp tools:", ", ".join(tool["name"] for tool in server.list_tools()))
        return
    server.serve_stdio()


if __name__ == "__main__":
    main()
