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

    mcp_tools = list_mcp_tools()
    expected_mcp_tools = {RPC_TO_MCP_TOOL[rpc] for rpc in proto_rpcs}
    if not expected_mcp_tools <= mcp_tools:
        errors.append(diff_message("MCP tools for proto RPCs", expected_mcp_tools, mcp_tools))

    client_methods = parse_client_methods()
    expected_client_methods = {RPC_TO_CLIENT_METHOD[rpc] for rpc in proto_rpcs}
    if not expected_client_methods <= client_methods:
        errors.append(
            diff_message("Python client methods for proto RPCs", expected_client_methods, client_methods)
        )

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


def list_mcp_tools() -> set[str]:
    from peekaboox.mcp import McpServer

    server = McpServer()
    server.register_default_tools()
    return {tool["name"] for tool in server.list_tools()}


def parse_client_methods() -> set[str]:
    tree = ast.parse((PYTHON_SRC / "peekaboox" / "client.py").read_text(encoding="utf-8"))
    client_class = next(
        node
        for node in tree.body
        if isinstance(node, ast.ClassDef) and node.name == "PeekabooXClient"
    )
    return {
        node.name
        for node in client_class.body
        if isinstance(node, ast.FunctionDef)
    }


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
