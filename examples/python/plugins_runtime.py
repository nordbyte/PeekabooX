#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

from peekaboox.agent import AgentRuntime
from peekaboox.plugins import PLUGIN_SDK_VERSION

PLUGIN_ID = "org.peekaboox.examples.system-info"
PLUGIN_TOOL = "system_info.uname"


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def main() -> int:
    root = repo_root()
    plugin_path = root / "examples" / "plugins"
    manifest_path = plugin_path / "system-info" / "peekaboox.plugin.json"
    runtime = AgentRuntime(plugin_paths=(plugin_path,))

    discovery = runtime.list_plugins()
    assert discovery.sdk_version == PLUGIN_SDK_VERSION
    assert not discovery.errors
    plugin = next((item for item in discovery.plugins if item.manifest.id == PLUGIN_ID), None)
    assert plugin is not None, f"missing plugin: {PLUGIN_ID}"
    assert plugin.manifest_path == manifest_path
    tool_names = {tool.name for tool in plugin.manifest.tools}
    assert PLUGIN_TOOL in tool_names

    explicit_discovery = runtime.list_plugins(paths=(manifest_path,))
    assert not explicit_discovery.errors
    assert explicit_discovery.plugins[0].manifest.id == PLUGIN_ID

    execution = runtime.call_plugin_tool(
        PLUGIN_ID,
        PLUGIN_TOOL,
        {},
        paths=(plugin_path,),
        timeout_seconds=5.0,
        max_output_bytes=65_536,
    )
    assert execution.ok, execution.error
    assert execution.exit_code == 0
    assert isinstance(execution.result, dict)
    required = {"system", "node", "release", "version", "machine", "processor"}
    assert required <= set(execution.result)

    summary = {
        "sdk_version": discovery.sdk_version,
        "plugin": PLUGIN_ID,
        "tool": PLUGIN_TOOL,
        "manifest_path": str(plugin.manifest_path.relative_to(root)),
        "result_keys": sorted(execution.result),
    }
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
