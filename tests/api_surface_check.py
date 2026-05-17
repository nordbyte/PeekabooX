#!/usr/bin/env python3
from __future__ import annotations

import ast
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
PYTHON_SRC = REPO_ROOT / "python" / "src"
if str(PYTHON_SRC) not in sys.path:
    sys.path.insert(0, str(PYTHON_SRC))


EXPECTED_CLI_COMMANDS = {
    "agent",
    "app",
    "capture",
    "capture-backends",
    "capture-delta",
    "capture-dmabuf",
    "clean",
    "click",
    "compare",
    "completions",
    "config",
    "desktop",
    "diagnose",
    "dialog",
    "dock",
    "doctor",
    "drag",
    "elements",
    "hotkey",
    "image",
    "launcher",
    "menu",
    "menubar",
    "move",
    "ocr",
    "paste",
    "perform-action",
    "permissions",
    "plugins",
    "plugin-call",
    "press",
    "scroll",
    "see",
    "set-value",
    "state",
    "swipe",
    "tools",
    "type",
    "vision-elements",
    "window",
    "windows",
    "workspace",
}

DOCUMENTED_COMMANDS = {
    "capture",
    "capture-backends",
    "capture-delta",
    "capture-dmabuf",
    "windows",
    "elements",
    "ocr",
    "compare",
    "state",
    "vision-elements",
    "desktop",
    "doctor",
    "diagnose",
    "click",
    "move",
    "drag",
    "type",
    "paste",
    "hotkey",
    "plugins",
    "plugin-call",
}

RPC_TO_MCP_TOOL = {
    "CaptureScreen": "capture_screen",
    "CaptureDelta": "capture_delta",
    "CaptureBackends": "capture_backends",
    "MoveMouse": "move_mouse",
    "Click": "click",
    "Drag": "drag",
    "TypeText": "type_text",
    "PasteText": "paste_text",
    "Hotkey": "hotkey",
    "FindElement": "find_element",
    "ListWindows": "list_windows",
    "GetDesktopState": "get_desktop_state",
    "OcrScreen": "ocr_screen",
    "CompareImages": "compare_images",
    "DetectUiState": "detect_ui_state",
    "DetectUiElements": "detect_ui_elements",
    "ProbeDmaBuf": "probe_dmabuf",
    "ListPlugins": "list_plugins",
    "CallPluginTool": "call_plugin_tool",
    "DesktopFocus": "desktop_focus",
    "DesktopLocate": "desktop_locate",
    "DesktopClick": "desktop_click",
    "DesktopDrag": "desktop_drag",
    "DesktopTypeInto": "desktop_type_into",
    "DesktopAssert": "desktop_assert",
    "DesktopProfiles": "desktop_profiles",
}

RPC_TO_CLIENT_METHOD = dict(RPC_TO_MCP_TOOL)
RPC_TO_CLIENT_METHOD.update(
    {
        "FindElement": "find_element",
        "DetectUiElements": "detect_ui_elements",
    }
)

CLIENT_PARAM_EXPECTATIONS = {
    "capture_screen": {"include_semantic_tree", "region", "window_id"},
    "capture_delta": {
        "stream_id",
        "reset",
        "region",
        "window_id",
        "per_channel_threshold",
        "low_bandwidth",
    },
    "capture_backends": {"output", "region", "diagnose", "probe"},
    "move_mouse": {
        "x",
        "y",
        "relative_x",
        "relative_y",
        "region",
        "ratio_x",
        "ratio_y",
        "window_id",
        "app",
        "window_title",
        "title_regex",
        "dry_run",
        "duration_ms",
        "steps",
        "bounds_policy",
        "backend",
        "restore",
    },
    "click": {
        "x",
        "y",
        "semantic_selector",
        "vision_fallback",
        "button",
        "dry_run",
        "bounds_policy",
        "backend",
        "restore",
        "region",
        "ratio_x",
        "ratio_y",
        "window_id",
        "app",
        "window_title",
        "title_regex",
    },
    "drag": {
        "from_x",
        "from_y",
        "to_x",
        "to_y",
        "button",
        "duration_ms",
        "dry_run",
        "steps",
        "bounds_policy",
        "backend",
        "restore",
        "from_current",
        "from_ratio",
        "to_ratio",
        "region",
        "window_id",
        "app",
        "window_title",
        "title_regex",
    },
    "type_text": {"text", "typing_speed_chars_per_second", "dry_run", "backend", "delay_ms", "key_delay_ms"},
    "paste_text": {
        "text",
        "preserve_clipboard",
        "dry_run",
        "clipboard_backend",
        "hotkey_backend",
        "delay_ms",
        "restore_delay_ms",
        "restore_policy",
    },
    "hotkey": {
        "keys",
        "dry_run",
        "backend",
        "delay_ms",
        "key_delay_ms",
        "repeat",
        "interval_ms",
        "release_before",
        "release_after",
    },
    "find_element": {
        "selector",
        "vision_fallback",
        "app",
        "window_title",
        "window_id",
        "vision_region",
        "vision_edge_threshold",
        "vision_min_width",
        "vision_min_height",
        "vision_min_component_pixels",
        "vision_max_elements",
        "vision_merge_distance",
    },
    "list_windows": {"id", "app", "title", "title_regex", "focused", "limit", "sort", "backend", "diagnose"},
    "ocr_screen": {
        "region",
        "language",
        "image_path",
        "window_id",
        "window_title",
        "app",
        "page_segmentation_mode",
        "engine_mode",
        "dpi",
        "min_confidence",
        "whitelist",
        "config",
        "scale",
        "grayscale",
        "threshold",
        "invert",
        "contrast",
        "deskew",
    },
    "compare_images": {
        "expected_image",
        "actual_image",
        "region",
        "ignore_regions",
        "per_channel_threshold",
        "max_changed_ratio",
        "max_changed_pixels",
        "max_mean_absolute_error",
        "max_channel_delta",
        "size_policy",
        "alpha",
    },
    "detect_ui_state": {
        "images",
        "region",
        "ignore_regions",
        "per_channel_threshold",
        "stable_max_changed_ratio",
        "stable_max_changed_pixels",
        "stable_max_mean_absolute_error",
        "stable_max_channel_delta",
        "loading_min_changed_ratio",
        "loading_min_changed_pixels",
        "required_stable_transitions",
        "size_policy",
        "alpha",
    },
    "detect_ui_elements": {
        "image",
        "region",
        "edge_threshold",
        "min_width",
        "min_height",
        "min_component_pixels",
        "max_elements",
        "merge_distance",
        "ignore_regions",
        "min_confidence",
        "max_width",
        "max_height",
        "min_area",
        "max_area",
        "padding",
        "sort",
        "mask_output_path",
        "overlay_output_path",
    },
    "probe_dmabuf": {"import_target"},
    "call_plugin_tool": {
        "plugin_id",
        "tool",
        "arguments",
        "paths",
        "timeout_seconds",
        "max_output_bytes",
        "require_trusted",
        "trust_policy",
    },
    "desktop_focus": {
        "app",
        "use_gnome_overview",
        "launch_if_needed",
        "wait_after_focus_ms",
        "overview_wait_ms",
        "window_title",
        "window_id",
        "verify",
    },
    "desktop_locate": {"app", "target", "image_path", "prefer_accessibility", "window_title", "window_id"},
    "desktop_click": {
        "app",
        "target",
        "image_path",
        "prefer_accessibility",
        "window_title",
        "window_id",
        "button",
        "dry_run",
        "verify",
    },
    "desktop_drag": {
        "app",
        "target",
        "image_path",
        "prefer_accessibility",
        "window_title",
        "window_id",
        "button",
        "from_ratio",
        "to_ratio",
        "duration_ms",
        "dry_run",
        "verify",
    },
    "desktop_type_into": {
        "app",
        "target",
        "text",
        "image_path",
        "prefer_accessibility",
        "window_title",
        "window_id",
        "clear",
        "dry_run",
        "verify",
    },
    "desktop_assert": {
        "app",
        "target",
        "assertion",
        "expected_text",
        "image_path",
        "prefer_accessibility",
        "window_title",
        "window_id",
    },
    "desktop_profiles": {"app", "target", "command", "desktop_id", "supports", "check", "installed", "available"},
}

MCP_PARAM_EXPECTATIONS = {
    **CLIENT_PARAM_EXPECTATIONS,
    "capture_screen": {
        "include_semantic_tree",
        "region",
        "window_id",
        "app",
        "window_title",
        "title_regex",
    },
    "click": CLIENT_PARAM_EXPECTATIONS["click"] | {"selector"},
    "drag": CLIENT_PARAM_EXPECTATIONS["drag"] - {"from_ratio", "to_ratio"} | {"from_ratio", "to_ratio"},
    "detect_ui_state": CLIENT_PARAM_EXPECTATIONS["detect_ui_state"] - {"images"} | {"image_paths"},
    "detect_ui_elements": CLIENT_PARAM_EXPECTATIONS["detect_ui_elements"] - {"image"} | {"image_path"},
    "compare_images": CLIENT_PARAM_EXPECTATIONS["compare_images"]
    - {"expected_image", "actual_image"}
    | {"expected_path", "actual_path"},
}


def main() -> int:
    errors: list[str] = []
    cli_commands = parse_cli_registry()
    if cli_commands != EXPECTED_CLI_COMMANDS:
        errors.append(diff_message("CLI registry", EXPECTED_CLI_COMMANDS, cli_commands))

    docs_text = (REPO_ROOT / "README.md").read_text(encoding="utf-8") + "\n" + (
        REPO_ROOT / "docs" / "cli.md"
    ).read_text(encoding="utf-8")
    missing_docs = sorted(
        command for command in DOCUMENTED_COMMANDS if command not in docs_text
    )
    if missing_docs:
        errors.append(f"documented command surface missing docs text: {', '.join(missing_docs)}")

    proto_rpcs = parse_proto_rpcs()
    missing_rpc_map = sorted(set(proto_rpcs) - set(RPC_TO_MCP_TOOL))
    if missing_rpc_map:
        errors.append(f"new proto RPCs need MCP/client mapping: {', '.join(missing_rpc_map)}")

    mcp_tool_properties = mcp_tool_input_properties()
    mcp_tools = set(mcp_tool_properties)
    expected_mcp_tools = {RPC_TO_MCP_TOOL[rpc] for rpc in proto_rpcs}
    if not expected_mcp_tools <= mcp_tools:
        errors.append(diff_message("MCP tools for proto RPCs", expected_mcp_tools, mcp_tools))

    client_method_params = parse_client_method_params()
    client_methods = set(client_method_params)
    expected_client_methods = {RPC_TO_CLIENT_METHOD[rpc] for rpc in proto_rpcs}
    if not expected_client_methods <= client_methods:
        errors.append(
            diff_message("Python client methods for proto RPCs", expected_client_methods, client_methods)
        )
    for method, expected_params in CLIENT_PARAM_EXPECTATIONS.items():
        actual = client_method_params.get(method, set())
        if not expected_params <= actual:
            errors.append(diff_message(f"Python client params {method}", expected_params, actual))
    for tool_name, expected_params in MCP_PARAM_EXPECTATIONS.items():
        actual = mcp_tool_properties.get(tool_name, set())
        if not expected_params <= actual:
            errors.append(diff_message(f"MCP input params {tool_name}", expected_params, actual))

    if errors:
        for error in errors:
            print(f"api-surface-check: {error}", file=sys.stderr)
        return 1
    print(
        "api-surface-check: ok "
        f"cli={len(cli_commands)} proto={len(proto_rpcs)} mcp={len(mcp_tools)} "
        f"client_methods={len(client_methods)}"
    )
    return 0


def parse_cli_registry() -> set[str]:
    text = (REPO_ROOT / "cli" / "src" / "legacy.rs").read_text(encoding="utf-8")
    match = re.search(
        r"fn cli_command_registry\(\) -> Vec<&'static str> \{\s*vec!\[(?P<body>.*?)\]\s*\}",
        text,
        re.S,
    )
    if not match:
        raise RuntimeError("could not find cli_command_registry")
    return set(re.findall(r'"([^"]+)"', match.group("body")))


def parse_proto_rpcs() -> list[str]:
    text = (REPO_ROOT / "proto" / "peekaboox" / "v1" / "peekaboox.proto").read_text(
        encoding="utf-8"
    )
    return re.findall(r"^\s*rpc\s+([A-Za-z0-9_]+)\(", text, re.M)


def mcp_tool_input_properties() -> dict[str, set[str]]:
    from peekaboox.mcp import McpServer

    server = McpServer()
    server.register_default_tools()
    return {
        tool["name"]: set(tool.get("inputSchema", {}).get("properties", {}))
        for tool in server.list_tools()
    }


def parse_client_method_params() -> dict[str, set[str]]:
    tree = ast.parse((PYTHON_SRC / "peekaboox" / "client.py").read_text(encoding="utf-8"))
    client_class = next(
        node
        for node in tree.body
        if isinstance(node, ast.ClassDef) and node.name == "PeekabooXClient"
    )
    methods = {}
    for node in client_class.body:
        if not isinstance(node, ast.FunctionDef):
            continue
        params = {
            arg.arg
            for arg in (
                node.args.posonlyargs
                + node.args.args
                + node.args.kwonlyargs
            )
            if arg.arg != "self"
        }
        methods[node.name] = params
    return methods


def diff_message(label: str, expected: set[str], actual: set[str]) -> str:
    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    parts = [label]
    if missing:
        parts.append(f"missing={','.join(missing)}")
    if extra:
        parts.append(f"extra={','.join(extra)}")
    return " ".join(parts)


if __name__ == "__main__":
    raise SystemExit(main())
