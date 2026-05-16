#!/usr/bin/env python3
from __future__ import annotations

import json
import tempfile
from pathlib import Path

from peekaboox.agent import AgentRuntime
from peekaboox.client import ActionResult
from peekaboox.workflows import Workflow, WorkflowStep, load_workflow_file, save_workflow_file


class RecordingClient:
    def __init__(self) -> None:
        self.calls: list[dict[str, object]] = []

    def paste_text(
        self,
        text: str,
        preserve_clipboard: bool = False,
        *,
        dry_run: bool = False,
        clipboard_backend: str | None = None,
        hotkey_backend: str | None = None,
        delay_ms: int | None = None,
        restore_delay_ms: int | None = None,
        restore_policy: str | None = None,
    ) -> ActionResult:
        self.calls.append(
            {
                "text": text,
                "preserve_clipboard": preserve_clipboard,
                "dry_run": dry_run,
                "clipboard_backend": clipboard_backend,
                "hotkey_backend": hotkey_backend,
                "delay_ms": delay_ms,
                "restore_delay_ms": restore_delay_ms,
                "restore_policy": restore_policy,
            }
        )
        return ActionResult(ok=True, message=f"would paste {len(text)} chars")


def main() -> int:
    client = RecordingClient()
    runtime = AgentRuntime(client=client)

    result = runtime.paste_text(
        "PeekabooX paste runtime example",
        preserve_clipboard=True,
        dry_run=True,
        clipboard_backend="auto",
        hotkey_backend="auto",
        delay_ms=80,
        restore_delay_ms=120,
        restore_policy="best-effort",
    )
    assert result.ok
    assert client.calls[-1]["preserve_clipboard"] is True
    assert client.calls[-1]["dry_run"] is True

    workflow = Workflow(
        name="paste-runtime-example",
        steps=[
            WorkflowStep(
                action="paste_text",
                value="PeekabooX workflow paste",
                preserve_clipboard=True,
                dry_run=True,
                clipboard_backend="auto",
                hotkey_backend="auto",
                delay_ms=80,
                restore_delay_ms=120,
                restore_policy="best-effort",
            )
        ],
    )
    with tempfile.TemporaryDirectory(prefix="peekaboox-paste-runtime-") as tmpdir:
        workflow_path = Path(tmpdir) / "paste.yaml"
        save_workflow_file(workflow, workflow_path, format_name="yaml")
        loaded = load_workflow_file(workflow_path)
        assert loaded.steps[0].clipboard_backend == "auto"
        assert loaded.steps[0].restore_policy == "best-effort"
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
