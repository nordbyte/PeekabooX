from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from peekaboox.workflows.io import dump_workflow_text, save_workflow_file
from peekaboox.workflows.model import Workflow, WorkflowStep


@dataclass(slots=True)
class WorkflowRecorder:
    name: str
    steps: list[WorkflowStep] = field(default_factory=list)
    enabled: bool = True

    def record_step(self, step: WorkflowStep) -> None:
        if self.enabled:
            self.steps.append(step)

    def workflow(self) -> Workflow:
        return Workflow(name=self.name, steps=list(self.steps))

    def clear(self) -> None:
        self.steps.clear()

    def to_json(self) -> str:
        return dump_workflow_text(self.workflow(), format_name="json")

    def to_yaml(self) -> str:
        return dump_workflow_text(self.workflow(), format_name="yaml")

    def save(self, path: str | Path, format_name: str | None = None) -> Path:
        return save_workflow_file(self.workflow(), path, format_name=format_name)
