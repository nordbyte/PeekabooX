from __future__ import annotations

from typing import Any

from peekaboox.agent.runtime import HOTKEY_BACKEND_CHOICES, TYPE_BACKEND_CHOICES

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


def schema(
    properties: dict[str, Any],
    required: list[str] | None = None,
    any_of: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    value: dict[str, Any] = {
        "type": "object",
        "properties": properties,
        "additionalProperties": False,
    }
    if required:
        value["required"] = required
    if any_of:
        value["anyOf"] = any_of
    return value


def ocr_input_schema() -> dict[str, Any]:
    return schema(
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


def tool_title(name: str) -> str:
    return name.replace("_", " ").title()


def tool_annotations(name: str) -> dict[str, Any]:
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
        "openWorldHint": name
        not in {"compare_images", "detect_ui_state", "detect_ui_elements", "vision_elements"},
    }


def tool_output_schema(name: str) -> dict[str, Any]:
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
    if name in {
        "find_element",
        "find_elements",
        "elements",
        "list_windows",
        "query_desktop_graph",
        "query_desktop_edges",
    }:
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
