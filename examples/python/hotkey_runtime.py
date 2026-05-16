#!/usr/bin/env python3
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from typing import Sequence

from peekaboox.agent import AgentRuntime
from peekaboox.client import ActionResult
from peekaboox.workflows import Workflow, WorkflowStep, load_workflow_file, save_workflow_file


class RecordingClient:
    def __init__(self) -> None:
        self.calls: list[dict[str, object]] = []

    def hotkey(
        self,
        keys: Sequence[str] | str,
        *,
        dry_run: bool = False,
        backend: str | None = None,
        delay_ms: int | None = None,
        key_delay_ms: int | None = None,
        repeat: int | None = None,
        interval_ms: int | None = None,
        release_before: bool = False,
        release_after: bool = False,
    ) -> ActionResult:
        key_values = [keys] if isinstance(keys, str) else list(keys)
        self.calls.append(
            {
                "keys": key_values,
                "dry_run": dry_run,
                "backend": backend,
                "delay_ms": delay_ms,
                "key_delay_ms": key_delay_ms,
                "repeat": repeat,
                "interval_ms": interval_ms,
                "release_before": release_before,
                "release_after": release_after,
            }
        )
        return ActionResult(ok=True, message="would press hotkey")


def main() -> int:
    client = RecordingClient()
    runtime = AgentRuntime(client=client)

    result = runtime.hotkey(
        "control+s",
        dry_run=True,
        backend="auto",
        delay_ms=25,
        key_delay_ms=30,
        repeat=2,
        interval_ms=40,
        release_before=True,
        release_after=True,
    )
    assert result.ok
    assert client.calls[-1]["keys"] == ["ctrl", "s"]
    assert client.calls[-1]["repeat"] == 2
    assert client.calls[-1]["release_after"] is True

    workflow = Workflow(
        name="hotkey-runtime-example",
        steps=[
            WorkflowStep(
                action="hotkey",
                value="control+s",
                dry_run=True,
                backend="auto",
                delay_ms=25,
                key_delay_ms=30,
                repeat=2,
                interval_ms=40,
                release_before=True,
                release_after=True,
                verify=False,
            )
        ],
    )
    with tempfile.TemporaryDirectory(prefix="peekaboox-hotkey-runtime-") as tmpdir:
        workflow_path = Path(tmpdir) / "hotkey.yaml"
        save_workflow_file(workflow, workflow_path, format_name="yaml")
        loaded = load_workflow_file(workflow_path)
        assert loaded.steps[0].backend == "auto"
        assert loaded.steps[0].repeat == 2
        assert loaded.steps[0].release_before is True
        runtime.execute_workflow(loaded)

    summary = {
        "calls": len(client.calls),
        "last_call": client.calls[-1],
        "message": result.message,
    }
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
