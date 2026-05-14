#!/usr/bin/env python3
from __future__ import annotations

import json
import tempfile
from pathlib import Path

from peekaboox.agent import AgentRuntime
from peekaboox.plugins import PLUGIN_SDK_VERSION
from peekaboox.workflows import Workflow, WorkflowStep, load_workflow_file, save_workflow_file


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def main() -> int:
    root = repo_root()
    runtime = AgentRuntime(plugin_paths=(root / "examples" / "plugins",))

    runtime.memory.put("example.goal", "inspect desktop without mutating state")
    assert runtime.memory.get("example.goal") == "inspect desktop without mutating state"

    plan = runtime.plan("Inspect the active desktop")
    assert plan == ["Inspect the active desktop"]

    generated = runtime.generate_workflow('Click the "Submit" button and type "Reviewed"')
    assert generated.steps
    assert generated.steps[0].action == "observe"

    workflow = Workflow(
        name="runtime-smoke",
        steps=[
            WorkflowStep(action="observe", verify=False),
            WorkflowStep(action="list_windows", verify=False),
            WorkflowStep(action="get_desktop_state", verify=False),
        ],
    )
    with tempfile.TemporaryDirectory(prefix="peekaboox-runtime-smoke-") as tmpdir:
        workflow_path = Path(tmpdir) / "workflow.yaml"
        save_workflow_file(workflow, workflow_path, format_name="yaml")
        loaded = load_workflow_file(workflow_path)
        assert loaded.name == workflow.name
        assert [step.action for step in loaded.steps] == [
            "observe",
            "list_windows",
            "get_desktop_state",
        ]

    plugins = runtime.list_plugins()
    assert plugins.sdk_version == PLUGIN_SDK_VERSION
    assert not plugins.errors
    plugin_ids = {plugin.manifest.id for plugin in plugins.plugins}
    assert "org.peekaboox.examples.system-info" in plugin_ids

    execution = runtime.call_plugin_tool(
        "org.peekaboox.examples.system-info",
        "system_info.uname",
        {},
    )
    assert execution.ok, execution.error
    assert isinstance(execution.result, dict)
    assert execution.result.get("system")

    summary = {
        "generated_steps": [step.action for step in generated.steps],
        "loaded_workflow": workflow.name,
        "plugins": sorted(plugin_ids),
        "plugin_result_keys": sorted(execution.result),
    }
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
