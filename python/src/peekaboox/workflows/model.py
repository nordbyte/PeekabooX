from dataclasses import dataclass, field, fields
from typing import Any

WORKFLOW_SCHEMA_VERSION = "peekaboox.workflow.v1"
LEGACY_WORKFLOW_SCHEMA_VERSIONS = {None, 1, "1", "workflow.v1", "peekaboox.workflow.legacy"}
SUPPORTED_WORKFLOW_ACTIONS = {
    "observe",
    "wait",
    "sleep",
    "assert",
    "assert_text",
    "capture",
    "capture_screen",
    "ocr",
    "ocr_screen",
    "compare",
    "compare_images",
    "detect_ui_state",
    "state",
    "detect_ui_elements",
    "vision_elements",
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
    "desktop_profiles",
    "desktop_focus",
    "desktop_locate",
    "desktop_click",
    "desktop_drag",
    "desktop_type_into",
    "desktop_assert",
    "plugin_call",
    "call_plugin_tool",
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
    target: str | None = None
    image_path: str | None = None
    expected_path: str | None = None
    actual_path: str | None = None
    image_paths: tuple[str, ...] = ()
    ignore_regions: tuple[str, ...] = ()
    language: str | None = None
    page_segmentation_mode: int | None = None
    engine_mode: int | None = None
    dpi: int | None = None
    min_confidence: float | None = None
    whitelist: str | None = None
    config: tuple[str, ...] = ()
    scale: float | None = None
    grayscale: bool = False
    threshold: int | None = None
    invert: bool = False
    contrast: float | None = None
    deskew: bool = False
    per_channel_threshold: int | None = None
    max_changed_ratio: float | None = None
    max_changed_pixels: int | None = None
    max_mean_absolute_error: float | None = None
    max_channel_delta: int | None = None
    stable_max_changed_ratio: float | None = None
    stable_max_changed_pixels: int | None = None
    stable_max_mean_absolute_error: float | None = None
    stable_max_channel_delta: int | None = None
    loading_min_changed_ratio: float | None = None
    loading_min_changed_pixels: int | None = None
    required_stable_transitions: int | None = None
    size_policy: str | None = None
    alpha: str | None = None
    edge_threshold: int | None = None
    min_width: int | None = None
    min_height: int | None = None
    min_component_pixels: int | None = None
    max_elements: int | None = None
    merge_distance: int | None = None
    max_width: int | None = None
    max_height: int | None = None
    min_area: int | None = None
    max_area: int | None = None
    padding: int | None = None
    sort: str | None = None
    mask_output_path: str | None = None
    overlay_output_path: str | None = None
    prefer_accessibility: bool = True
    use_gnome_overview: bool = True
    launch_if_needed: bool = True
    wait_after_focus_ms: int | None = None
    overview_wait_ms: int | None = None
    clear: bool = False
    assertion: str | None = None
    expected_text: str | None = None
    plugin_id: str | None = None
    tool: str | None = None
    arguments: dict[str, object] | None = None
    timeout_seconds: float | None = None
    max_output_bytes: int | None = None
    require_trusted: bool | None = None
    trust_policy: str | None = None
    sleep_ms: int | None = None


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
    for name in (
        "duration_ms",
        "delay_ms",
        "key_delay_ms",
        "interval_ms",
        "restore_delay_ms",
        "wait_after_focus_ms",
        "overview_wait_ms",
        "sleep_ms",
    ):
        value = getattr(step, name)
        if value is not None and value < 0:
            raise WorkflowValidationError(f"steps[{index}].{name} must be non-negative")
    for name in (
        "page_segmentation_mode",
        "engine_mode",
        "per_channel_threshold",
        "max_changed_pixels",
        "max_channel_delta",
        "stable_max_changed_pixels",
        "stable_max_channel_delta",
        "loading_min_changed_pixels",
        "merge_distance",
        "padding",
    ):
        value = getattr(step, name)
        if value is not None and value < 0:
            raise WorkflowValidationError(f"steps[{index}].{name} must be non-negative")
    for name in (
        "steps",
        "typing_speed_chars_per_second",
        "repeat",
        "dpi",
        "required_stable_transitions",
        "edge_threshold",
        "min_width",
        "min_height",
        "min_component_pixels",
        "max_elements",
        "max_width",
        "max_height",
        "min_area",
        "max_area",
        "max_output_bytes",
    ):
        value = getattr(step, name)
        if value is not None and value < 1:
            raise WorkflowValidationError(f"steps[{index}].{name} must be positive")
    for name in (
        "min_confidence",
        "max_changed_ratio",
        "stable_max_changed_ratio",
        "loading_min_changed_ratio",
    ):
        value = getattr(step, name)
        if value is not None and not 0.0 <= value <= 1.0:
            raise WorkflowValidationError(f"steps[{index}].{name} must be between 0.0 and 1.0")
    for name in ("max_mean_absolute_error", "stable_max_mean_absolute_error"):
        value = getattr(step, name)
        if value is not None and value < 0:
            raise WorkflowValidationError(f"steps[{index}].{name} must be non-negative")
    if step.contrast is not None and not -255.0 <= step.contrast <= 255.0:
        raise WorkflowValidationError(f"steps[{index}].contrast must be between -255.0 and 255.0")
    if step.timeout_seconds is not None and step.timeout_seconds <= 0:
        raise WorkflowValidationError(f"steps[{index}].timeout_seconds must be positive")
    if action in {"wait", "sleep"} and step.sleep_ms is None and step.duration_ms is None:
        raise WorkflowValidationError(f"steps[{index}] {action} requires sleep_ms or duration_ms")
    if action in {"assert", "assert_text"} and not (step.selector or step.expected_text or step.value):
        raise WorkflowValidationError(
            f"steps[{index}] {action} requires selector, expected_text, or value"
        )
    if action == "find_element" and not step.selector:
        raise WorkflowValidationError(f"steps[{index}] find_element requires selector")
    if action in {"ocr", "ocr_screen"} and step.image_path is None:
        has_scope = any(
            value is not None
            for value in (step.region, step.window_id, step.app, step.window_title, step.title_regex)
        )
        if step.region is None and not has_scope:
            pass
    if action in {"compare", "compare_images"} and not (step.expected_path and step.actual_path):
        raise WorkflowValidationError(
            f"steps[{index}] {action} requires expected_path and actual_path"
        )
    if action in {"detect_ui_state", "state"} and len(step.image_paths) < 2:
        raise WorkflowValidationError(f"steps[{index}] {action} requires at least two image_paths")
    if action in {"detect_ui_elements", "vision_elements"} and not step.image_path:
        raise WorkflowValidationError(f"steps[{index}] {action} requires image_path")
    if action.startswith("desktop_"):
        if action != "desktop_profiles" and not step.app:
            raise WorkflowValidationError(f"steps[{index}] {action} requires app")
        if action not in {"desktop_profiles", "desktop_focus"} and not step.target:
            raise WorkflowValidationError(f"steps[{index}] {action} requires target")
        if action == "desktop_type_into" and step.value is None:
            raise WorkflowValidationError(f"steps[{index}] desktop_type_into requires value")
    if action in {"plugin_call", "call_plugin_tool"} and not (step.plugin_id and step.tool):
        raise WorkflowValidationError(f"steps[{index}] {action} requires plugin_id and tool")
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
        "target": {"type": "string"},
        "image_path": {"type": "string"},
        "expected_path": {"type": "string"},
        "actual_path": {"type": "string"},
        "image_paths": {"type": "array", "items": {"type": "string"}},
        "ignore_regions": {"type": "array", "items": {"type": "string"}},
        "language": {"type": "string"},
        "page_segmentation_mode": {"type": "integer", "minimum": 0},
        "engine_mode": {"type": "integer", "minimum": 0},
        "dpi": {"type": "integer", "minimum": 1},
        "min_confidence": {"type": "number", "minimum": 0, "maximum": 1},
        "whitelist": {"type": "string"},
        "config": {"type": "array", "items": {"type": "string"}},
        "scale": {"type": "number", "minimum": 0.1},
        "grayscale": {"type": "boolean"},
        "threshold": {"type": "integer", "minimum": 0, "maximum": 255},
        "invert": {"type": "boolean"},
        "contrast": {"type": "number", "minimum": -255, "maximum": 255},
        "deskew": {"type": "boolean"},
        "per_channel_threshold": {"type": "integer", "minimum": 0},
        "max_changed_ratio": {"type": "number", "minimum": 0, "maximum": 1},
        "max_changed_pixels": {"type": "integer", "minimum": 0},
        "max_mean_absolute_error": {"type": "number", "minimum": 0},
        "max_channel_delta": {"type": "integer", "minimum": 0},
        "stable_max_changed_ratio": {"type": "number", "minimum": 0, "maximum": 1},
        "stable_max_changed_pixels": {"type": "integer", "minimum": 0},
        "stable_max_mean_absolute_error": {"type": "number", "minimum": 0},
        "stable_max_channel_delta": {"type": "integer", "minimum": 0},
        "loading_min_changed_ratio": {"type": "number", "minimum": 0, "maximum": 1},
        "loading_min_changed_pixels": {"type": "integer", "minimum": 0},
        "required_stable_transitions": {"type": "integer", "minimum": 1},
        "size_policy": {"type": "string"},
        "alpha": {"type": "string"},
        "edge_threshold": {"type": "integer", "minimum": 1},
        "min_width": {"type": "integer", "minimum": 1},
        "min_height": {"type": "integer", "minimum": 1},
        "min_component_pixels": {"type": "integer", "minimum": 1},
        "max_elements": {"type": "integer", "minimum": 1},
        "merge_distance": {"type": "integer", "minimum": 0},
        "max_width": {"type": "integer", "minimum": 1},
        "max_height": {"type": "integer", "minimum": 1},
        "min_area": {"type": "integer", "minimum": 1},
        "max_area": {"type": "integer", "minimum": 1},
        "padding": {"type": "integer", "minimum": 0},
        "sort": {"type": "string"},
        "mask_output_path": {"type": "string"},
        "overlay_output_path": {"type": "string"},
        "prefer_accessibility": {"type": "boolean"},
        "use_gnome_overview": {"type": "boolean"},
        "launch_if_needed": {"type": "boolean"},
        "wait_after_focus_ms": {"type": "integer", "minimum": 0},
        "overview_wait_ms": {"type": "integer", "minimum": 0},
        "clear": {"type": "boolean"},
        "assertion": {"type": "string"},
        "expected_text": {"type": "string"},
        "plugin_id": {"type": "string"},
        "tool": {"type": "string"},
        "arguments": {"type": "object"},
        "timeout_seconds": {"type": "number", "minimum": 0},
        "max_output_bytes": {"type": "integer", "minimum": 1},
        "require_trusted": {"type": "boolean"},
        "trust_policy": {"type": "string"},
        "sleep_ms": {"type": "integer", "minimum": 0},
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
