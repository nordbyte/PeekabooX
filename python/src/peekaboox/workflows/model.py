from dataclasses import dataclass, field


@dataclass(frozen=True, slots=True)
class WorkflowStep:
    action: str
    selector: str | None = None
    value: str | None = None
    x: int | None = None
    y: int | None = None
    from_x: int | None = None
    from_y: int | None = None
    to_x: int | None = None
    to_y: int | None = None
    button: str | None = None
    duration_ms: int | None = None
    vision_fallback: bool = False
    verify: bool = True


@dataclass(slots=True)
class Workflow:
    name: str
    steps: list[WorkflowStep] = field(default_factory=list)

    def add_step(self, step: WorkflowStep) -> None:
        self.steps.append(step)
