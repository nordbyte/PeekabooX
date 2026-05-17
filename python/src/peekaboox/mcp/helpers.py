from __future__ import annotations

import argparse
import base64
import hmac
import ipaddress
import json
import os
from dataclasses import fields, is_dataclass
from pathlib import Path
from typing import Any

from peekaboox.agent import PreflightError, WorkflowExecutionResult
from peekaboox.agent.runtime import WINDOW_BACKEND_CHOICES, WINDOW_SORT_CHOICES
from peekaboox.client import Rect
from peekaboox.mcp.types import JsonRpcProtocolError, McpPrompt
from peekaboox.security import (
    CapabilityDeniedError,
    ConfirmationDeniedError,
    ConfirmationRequiredError,
)
from peekaboox.workflows import workflow_from_dict

INVALID_PARAMS = -32602

__all__ = ['_optional_rect', '_optional_rects', '_required_str', '_required_object', '_required_string_array', '_optional_int', '_optional_float', '_positive_float', '_positive_int', '_env_positive_float', '_env_positive_int', '_optional_bool', '_optional_positive_int', '_optional_nonnegative_int', '_optional_choice', '_window_query_kwargs_from_arguments', '_optional_ratio_pair', '_optional_string', '_optional_workflow', '_to_mcp_value', '_preflight_error_content', '_capability_error_content', '_confirmation_error_content', '_generic_error_content', '_tool_result_content', '_json_resource', '_text_resource', '_repo_root', '_read_repo_text', '_completion_context_value', '_completion_matches', '_render_prompt', '_workflow_execution_result_from_mcp', '_jsonrpc_error', '_normalize_mcp_auth_token', '_mcp_http_host_requires_auth', '_mcp_http_request_authorized', '_header_value', '_mcp_http_content_length']


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


def _positive_int(value: str) -> int:
    parsed = int(value)
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


def _env_positive_int(name: str, default: int) -> int:
    value = os.environ.get(name)
    if value is None:
        return default
    parsed = int(value)
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

def _jsonrpc_error(request_id: Any, code: int, message: str) -> dict[str, Any]:
    return {
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {"code": code, "message": message},
    }


def _normalize_mcp_auth_token(token: str | None) -> str | None:
    if token is None:
        return None
    token = token.strip()
    return token or None


def _mcp_http_host_requires_auth(host: str) -> bool:
    host = host.strip().strip("[]").casefold()
    if host in {"localhost"}:
        return False
    if not host:
        return True
    try:
        return not ipaddress.ip_address(host).is_loopback
    except ValueError:
        return True


def _mcp_http_request_authorized(headers: Any, auth_token: str | None) -> bool:
    if auth_token is None:
        return True
    bearer = _header_value(headers, "authorization")
    if bearer is not None:
        scheme, _, value = bearer.partition(" ")
        if scheme.casefold() == "bearer" and hmac.compare_digest(value.strip(), auth_token):
            return True
    for header in ("x-peekaboox-mcp-token", "x-peekaboox-token"):
        value = _header_value(headers, header)
        if value is not None and hmac.compare_digest(value.strip(), auth_token):
            return True
    return False


def _header_value(headers: Any, name: str) -> str | None:
    getter = getattr(headers, "get", None)
    if getter is None:
        return None
    candidates = (name, name.casefold(), "-".join(part.capitalize() for part in name.split("-")))
    value = None
    for candidate in candidates:
        value = getter(candidate)
        if value is not None:
            break
    if value is None:
        items = getattr(headers, "items", None)
        if items is not None:
            for key, candidate_value in items():
                if str(key).casefold() == name.casefold():
                    value = candidate_value
                    break
    return None if value is None else str(value)


def _mcp_http_content_length(value: str | None, max_request_bytes: int) -> int:
    if value is None:
        raise ValueError("missing content length")
    try:
        length = int(value)
    except ValueError as error:
        raise ValueError("invalid content length") from error
    if length < 0:
        raise ValueError("invalid content length")
    if length > max_request_bytes:
        raise OverflowError("request body too large")
    return length
