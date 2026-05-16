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
    relative_x: int | None = None
    relative_y: int | None = None
    region: str | None = None
    ratio_x: float | None = None
    ratio_y: float | None = None
    window_id: str | None = None
    app: str | None = None
    window_title: str | None = None
    title_regex: str | None = None
    steps: int | None = None
    bounds_policy: str | None = None
    backend: str | None = None
    restore: bool = False
    dry_run: bool = False
    vision_fallback: bool = False
    verify: bool = True


@dataclass(slots=True)
class Workflow:
    name: str
    steps: list[WorkflowStep] = field(default_factory=list)

    def add_step(self, step: WorkflowStep) -> None:
        self.steps.append(step)
