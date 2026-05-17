from dataclasses import dataclass, field, fields
from typing import Any

WORKFLOW_SCHEMA_VERSION = "peekaboox.workflow.v1"
LEGACY_WORKFLOW_SCHEMA_VERSIONS = {None, 1, "1", "workflow.v1", "peekaboox.workflow.legacy"}
SUPPORTED_WORKFLOW_ACTIONS = {
    "observe",
    "capture",
    "capture_screen",
    "find_element",
    "click",
    "move",
    "move_mouse",
    "drag",
    "type",
    "type_text",
    "paste",
    "paste_text",
    "hotkey",
    "list_windows",
    "get_desktop_state",
}


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
    from_current: bool = False
    from_ratio_x: float | None = None
    from_ratio_y: float | None = None
    to_ratio_x: float | None = None
    to_ratio_y: float | None = None
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
    clipboard_backend: str | None = None
    hotkey_backend: str | None = None
    typing_speed_chars_per_second: int | None = None
    delay_ms: int | None = None
    key_delay_ms: int | None = None
    repeat: int | None = None
    interval_ms: int | None = None
    release_before: bool = False
    release_after: bool = False
    preserve_clipboard: bool = False
    restore_delay_ms: int | None = None
    restore_policy: str | None = None
    restore: bool = False
    dry_run: bool = False
    vision_fallback: bool = False
    verify: bool = True


@dataclass(slots=True)
class Workflow:
    name: str
    steps: list[WorkflowStep] = field(default_factory=list)
    schema_version: str = WORKFLOW_SCHEMA_VERSION

    def add_step(self, step: WorkflowStep) -> None:
        validate_workflow_step(step, index=len(self.steps))
        self.steps.append(step)


class WorkflowValidationError(ValueError):
    pass


def workflow_step_fields() -> set[str]:
    return {field.name for field in fields(WorkflowStep)}


def normalize_workflow_schema_version(value: object) -> str:
    if value == WORKFLOW_SCHEMA_VERSION:
        return WORKFLOW_SCHEMA_VERSION
    if value in LEGACY_WORKFLOW_SCHEMA_VERSIONS:
        return WORKFLOW_SCHEMA_VERSION
    raise WorkflowValidationError(
        f"unsupported workflow schema_version {value!r}; expected {WORKFLOW_SCHEMA_VERSION!r}"
    )


def validate_workflow(workflow: Workflow) -> None:
    normalize_workflow_schema_version(workflow.schema_version)
    if not workflow.name.strip():
        raise WorkflowValidationError("workflow name must be a non-empty string")
    if not workflow.steps:
        raise WorkflowValidationError("workflow steps must be a non-empty list")
    for index, step in enumerate(workflow.steps):
        validate_workflow_step(step, index=index)


def validate_workflow_step(step: WorkflowStep, index: int = 0) -> None:
    action = step.action.strip().lower()
    if not action:
        raise WorkflowValidationError(f"steps[{index}].action must be a non-empty string")
    if action not in SUPPORTED_WORKFLOW_ACTIONS:
        raise WorkflowValidationError(f"steps[{index}].action {step.action!r} is not supported")
    for name in (
        "from_ratio_x",
        "from_ratio_y",
        "to_ratio_x",
        "to_ratio_y",
        "ratio_x",
        "ratio_y",
    ):
        value = getattr(step, name)
        if value is not None and not 0.0 <= value <= 1.0:
            raise WorkflowValidationError(f"steps[{index}].{name} must be between 0.0 and 1.0")
    for name in ("duration_ms", "delay_ms", "key_delay_ms", "interval_ms", "restore_delay_ms"):
        value = getattr(step, name)
        if value is not None and value < 0:
            raise WorkflowValidationError(f"steps[{index}].{name} must be non-negative")
    for name in ("steps", "typing_speed_chars_per_second", "repeat"):
        value = getattr(step, name)
        if value is not None and value < 1:
            raise WorkflowValidationError(f"steps[{index}].{name} must be positive")
    if action == "find_element" and not step.selector:
        raise WorkflowValidationError(f"steps[{index}] find_element requires selector")
    if action == "click":
        has_coordinates = step.x is not None or step.y is not None
        has_scope = any(
            value is not None
            for value in (
                step.region,
                step.ratio_x,
                step.ratio_y,
                step.window_id,
                step.app,
                step.window_title,
                step.title_regex,
            )
        )
        if has_coordinates and (step.x is None or step.y is None):
            raise WorkflowValidationError(f"steps[{index}] click x/y target is incomplete")
        if not step.selector and not has_coordinates and not has_scope:
            raise WorkflowValidationError(
                f"steps[{index}] click requires selector, x/y coordinates, or a scope"
            )
    if action in {"move", "move_mouse"}:
        has_coordinates = step.x is not None or step.y is not None
        has_relative = step.relative_x is not None or step.relative_y is not None
        has_scope = step.ratio_x is not None or step.ratio_y is not None
        if has_coordinates and (step.x is None or step.y is None):
            raise WorkflowValidationError(f"steps[{index}] move_mouse x/y target is incomplete")
        if has_relative and (step.relative_x is None or step.relative_y is None):
            raise WorkflowValidationError(
                f"steps[{index}] move_mouse relative target is incomplete"
            )
        if has_scope and (step.ratio_x is None or step.ratio_y is None):
            raise WorkflowValidationError(f"steps[{index}] move_mouse ratio target is incomplete")
    if action == "drag":
        has_absolute = any(
            value is not None for value in (step.from_x, step.from_y, step.to_x, step.to_y)
        )
        if has_absolute and any(
            value is None for value in (step.from_x, step.from_y, step.to_x, step.to_y)
        ):
            raise WorkflowValidationError(f"steps[{index}] drag absolute target is incomplete")
        has_from_ratio = step.from_ratio_x is not None or step.from_ratio_y is not None
        has_to_ratio = step.to_ratio_x is not None or step.to_ratio_y is not None
        if has_from_ratio and (step.from_ratio_x is None or step.from_ratio_y is None):
            raise WorkflowValidationError(f"steps[{index}] drag from_ratio target is incomplete")
        if has_to_ratio and (step.to_ratio_x is None or step.to_ratio_y is None):
            raise WorkflowValidationError(f"steps[{index}] drag to_ratio target is incomplete")
        if not has_absolute and not step.from_current and not (has_from_ratio and has_to_ratio):
            raise WorkflowValidationError(
                f"steps[{index}] drag requires absolute points, from_current, or ratios"
            )
    if action in {"type", "type_text", "paste", "paste_text", "hotkey"} and step.value is None:
        raise WorkflowValidationError(f"steps[{index}] {action} requires value")


def workflow_json_schema() -> dict[str, Any]:
    step_properties = {
        "action": {"type": "string", "enum": sorted(SUPPORTED_WORKFLOW_ACTIONS)},
        "selector": {"type": "string"},
        "value": {"type": "string"},
        "x": {"type": "integer"},
        "y": {"type": "integer"},
        "from_x": {"type": "integer"},
        "from_y": {"type": "integer"},
        "to_x": {"type": "integer"},
        "to_y": {"type": "integer"},
        "from_current": {"type": "boolean"},
        "from_ratio_x": {"type": "number", "minimum": 0, "maximum": 1},
        "from_ratio_y": {"type": "number", "minimum": 0, "maximum": 1},
        "to_ratio_x": {"type": "number", "minimum": 0, "maximum": 1},
        "to_ratio_y": {"type": "number", "minimum": 0, "maximum": 1},
        "button": {"type": "string"},
        "duration_ms": {"type": "integer", "minimum": 0},
        "relative_x": {"type": "integer"},
        "relative_y": {"type": "integer"},
        "region": {"type": "string"},
        "ratio_x": {"type": "number", "minimum": 0, "maximum": 1},
        "ratio_y": {"type": "number", "minimum": 0, "maximum": 1},
        "window_id": {"type": "string"},
        "app": {"type": "string"},
        "window_title": {"type": "string"},
        "title_regex": {"type": "string"},
        "steps": {"type": "integer", "minimum": 1},
        "bounds_policy": {"type": "string"},
        "backend": {"type": "string"},
        "clipboard_backend": {"type": "string"},
        "hotkey_backend": {"type": "string"},
        "typing_speed_chars_per_second": {"type": "integer", "minimum": 1},
        "delay_ms": {"type": "integer", "minimum": 0},
        "key_delay_ms": {"type": "integer", "minimum": 0},
        "repeat": {"type": "integer", "minimum": 1},
        "interval_ms": {"type": "integer", "minimum": 0},
        "release_before": {"type": "boolean"},
        "release_after": {"type": "boolean"},
        "preserve_clipboard": {"type": "boolean"},
        "restore_delay_ms": {"type": "integer", "minimum": 0},
        "restore_policy": {"type": "string"},
        "restore": {"type": "boolean"},
        "dry_run": {"type": "boolean"},
        "vision_fallback": {"type": "boolean"},
        "verify": {"type": "boolean"},
    }
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://github.com/nordbyte/PeekabooX/schemas/workflow.v1.json",
        "title": "PeekabooX Workflow",
        "type": "object",
        "additionalProperties": False,
        "required": ["schema_version", "name", "steps"],
        "properties": {
            "schema_version": {"const": WORKFLOW_SCHEMA_VERSION},
            "name": {"type": "string", "minLength": 1},
            "steps": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": False,
                    "required": ["action"],
                    "properties": step_properties,
                },
            },
        },
    }
