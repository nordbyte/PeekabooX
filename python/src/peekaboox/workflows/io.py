from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from peekaboox.workflows.model import (
    Workflow,
    WorkflowStep,
    normalize_workflow_schema_version,
    validate_workflow,
    validate_workflow_step,
    workflow_step_fields,
)


def load_workflow_file(path: str | Path) -> Workflow:
    workflow_path = Path(path)
    text = workflow_path.read_text(encoding="utf-8")
    suffix = workflow_path.suffix.casefold()
    if suffix == ".json":
        return load_workflow_text(text, format_name="json")
    if suffix in {".yaml", ".yml"}:
        return load_workflow_text(text, format_name="yaml")
    return load_workflow_text(text)


def save_workflow_file(
    workflow: Workflow,
    path: str | Path,
    format_name: str | None = None,
) -> Path:
    workflow_path = Path(path)
    format_name = _detect_output_format(workflow_path, format_name)
    workflow_path.parent.mkdir(parents=True, exist_ok=True)
    workflow_path.write_text(
        dump_workflow_text(workflow, format_name=format_name),
        encoding="utf-8",
    )
    return workflow_path


def load_workflow_text(text: str, format_name: str | None = None) -> Workflow:
    format_name = _detect_format(text, format_name)
    if format_name == "json":
        decoded = json.loads(text)
    elif format_name == "yaml":
        decoded = _parse_simple_yaml(text)
    else:
        raise ValueError(f"unsupported workflow format: {format_name}")
    if not isinstance(decoded, dict):
        raise ValueError("workflow definition must be an object")
    return workflow_from_dict(decoded)


def dump_workflow_text(workflow: Workflow, format_name: str = "json") -> str:
    format_name = format_name.casefold()
    if format_name == "json":
        return json.dumps(workflow_to_dict(workflow), indent=2, sort_keys=True) + "\n"
    if format_name == "yaml":
        return _dump_simple_yaml(workflow)
    raise ValueError(f"unsupported workflow format: {format_name}")


def workflow_from_dict(value: dict[str, object], default_name: str = "workflow") -> Workflow:
    unknown = set(value) - {"schema_version", "name", "steps"}
    if unknown:
        raise ValueError(f"workflow has unsupported top-level keys: {', '.join(sorted(unknown))}")
    schema_version = normalize_workflow_schema_version(value.get("schema_version"))
    name = value.get("name", default_name)
    if not isinstance(name, str) or not name.strip():
        raise ValueError("workflow name must be a non-empty string")
    steps = value.get("steps")
    if not isinstance(steps, list) or not steps:
        raise ValueError("workflow steps must be a non-empty list")
    workflow = Workflow(
        name=name,
        steps=[workflow_step_from_dict(step, index) for index, step in enumerate(steps)],
        schema_version=schema_version,
    )
    validate_workflow(workflow)
    return workflow


def workflow_to_dict(workflow: Workflow) -> dict[str, object]:
    validate_workflow(workflow)
    return {
        "schema_version": workflow.schema_version,
        "name": workflow.name,
        "steps": [workflow_step_to_dict(step) for step in workflow.steps],
    }


def workflow_step_from_dict(value: Any, index: int = 0) -> WorkflowStep:
    if not isinstance(value, dict):
        raise ValueError(f"steps[{index}] must be an object")
    unknown = set(value) - workflow_step_fields()
    if unknown:
        raise ValueError(f"steps[{index}] has unsupported keys: {', '.join(sorted(unknown))}")
    action = value.get("action")
    if not isinstance(action, str) or not action.strip():
        raise ValueError(f"steps[{index}].action must be a non-empty string")
    step = WorkflowStep(
        action=action,
        selector=_optional_string(value, "selector"),
        value=_optional_string(value, "value"),
        x=_optional_int(value, "x"),
        y=_optional_int(value, "y"),
        from_x=_optional_int(value, "from_x"),
        from_y=_optional_int(value, "from_y"),
        to_x=_optional_int(value, "to_x"),
        to_y=_optional_int(value, "to_y"),
        from_current=_optional_bool(value, "from_current", default=False),
        from_ratio_x=_optional_float(value, "from_ratio_x"),
        from_ratio_y=_optional_float(value, "from_ratio_y"),
        to_ratio_x=_optional_float(value, "to_ratio_x"),
        to_ratio_y=_optional_float(value, "to_ratio_y"),
        button=_optional_string(value, "button"),
        duration_ms=_optional_int(value, "duration_ms"),
        relative_x=_optional_int(value, "relative_x"),
        relative_y=_optional_int(value, "relative_y"),
        region=_optional_string(value, "region"),
        ratio_x=_optional_float(value, "ratio_x"),
        ratio_y=_optional_float(value, "ratio_y"),
        window_id=_optional_string(value, "window_id"),
        app=_optional_string(value, "app"),
        window_title=_optional_string(value, "window_title"),
        title_regex=_optional_string(value, "title_regex"),
        steps=_optional_int(value, "steps"),
        bounds_policy=_optional_string(value, "bounds_policy"),
        backend=_optional_string(value, "backend"),
        clipboard_backend=_optional_string(value, "clipboard_backend"),
        hotkey_backend=_optional_string(value, "hotkey_backend"),
        typing_speed_chars_per_second=_optional_int(
            value, "typing_speed_chars_per_second"
        ),
        delay_ms=_optional_int(value, "delay_ms"),
        key_delay_ms=_optional_int(value, "key_delay_ms"),
        repeat=_optional_int(value, "repeat"),
        interval_ms=_optional_int(value, "interval_ms"),
        release_before=_optional_bool(value, "release_before", default=False),
        release_after=_optional_bool(value, "release_after", default=False),
        preserve_clipboard=_optional_bool(value, "preserve_clipboard", default=False),
        restore_delay_ms=_optional_int(value, "restore_delay_ms"),
        restore_policy=_optional_string(value, "restore_policy"),
        restore=_optional_bool(value, "restore", default=False),
        dry_run=_optional_bool(value, "dry_run", default=False),
        vision_fallback=_optional_bool(value, "vision_fallback", default=False),
        verify=_optional_bool(value, "verify", default=True),
        target=_optional_string(value, "target"),
        image_path=_optional_string(value, "image_path"),
        expected_path=_optional_string(value, "expected_path"),
        actual_path=_optional_string(value, "actual_path"),
        image_paths=_optional_string_tuple(value, "image_paths"),
        ignore_regions=_optional_string_tuple(value, "ignore_regions"),
        language=_optional_string(value, "language"),
        page_segmentation_mode=_optional_int(value, "page_segmentation_mode"),
        engine_mode=_optional_int(value, "engine_mode"),
        dpi=_optional_int(value, "dpi"),
        min_confidence=_optional_float(value, "min_confidence"),
        whitelist=_optional_string(value, "whitelist"),
        config=_optional_string_tuple(value, "config"),
        scale=_optional_float(value, "scale"),
        grayscale=_optional_bool(value, "grayscale", default=False),
        threshold=_optional_int(value, "threshold"),
        invert=_optional_bool(value, "invert", default=False),
        contrast=_optional_float(value, "contrast"),
        deskew=_optional_bool(value, "deskew", default=False),
        per_channel_threshold=_optional_int(value, "per_channel_threshold"),
        max_changed_ratio=_optional_float(value, "max_changed_ratio"),
        max_changed_pixels=_optional_int(value, "max_changed_pixels"),
        max_mean_absolute_error=_optional_float(value, "max_mean_absolute_error"),
        max_channel_delta=_optional_int(value, "max_channel_delta"),
        stable_max_changed_ratio=_optional_float(value, "stable_max_changed_ratio"),
        stable_max_changed_pixels=_optional_int(value, "stable_max_changed_pixels"),
        stable_max_mean_absolute_error=_optional_float(value, "stable_max_mean_absolute_error"),
        stable_max_channel_delta=_optional_int(value, "stable_max_channel_delta"),
        loading_min_changed_ratio=_optional_float(value, "loading_min_changed_ratio"),
        loading_min_changed_pixels=_optional_int(value, "loading_min_changed_pixels"),
        required_stable_transitions=_optional_int(value, "required_stable_transitions"),
        size_policy=_optional_string(value, "size_policy"),
        alpha=_optional_string(value, "alpha"),
        edge_threshold=_optional_int(value, "edge_threshold"),
        min_width=_optional_int(value, "min_width"),
        min_height=_optional_int(value, "min_height"),
        min_component_pixels=_optional_int(value, "min_component_pixels"),
        max_elements=_optional_int(value, "max_elements"),
        merge_distance=_optional_int(value, "merge_distance"),
        max_width=_optional_int(value, "max_width"),
        max_height=_optional_int(value, "max_height"),
        min_area=_optional_int(value, "min_area"),
        max_area=_optional_int(value, "max_area"),
        padding=_optional_int(value, "padding"),
        sort=_optional_string(value, "sort"),
        mask_output_path=_optional_string(value, "mask_output_path"),
        overlay_output_path=_optional_string(value, "overlay_output_path"),
        prefer_accessibility=_optional_bool(value, "prefer_accessibility", default=True),
        use_gnome_overview=_optional_bool(value, "use_gnome_overview", default=True),
        launch_if_needed=_optional_bool(value, "launch_if_needed", default=True),
        wait_after_focus_ms=_optional_int(value, "wait_after_focus_ms"),
        overview_wait_ms=_optional_int(value, "overview_wait_ms"),
        clear=_optional_bool(value, "clear", default=False),
        assertion=_optional_string(value, "assertion"),
        expected_text=_optional_string(value, "expected_text"),
        plugin_id=_optional_string(value, "plugin_id"),
        tool=_optional_string(value, "tool"),
        arguments=_optional_object(value, "arguments"),
        timeout_seconds=_optional_float(value, "timeout_seconds"),
        max_output_bytes=_optional_int(value, "max_output_bytes"),
        require_trusted=_optional_bool_or_none(value, "require_trusted"),
        trust_policy=_optional_string(value, "trust_policy"),
        sleep_ms=_optional_int(value, "sleep_ms"),
    )
    validate_workflow_step(step, index=index)
    return step


def workflow_step_to_dict(step: WorkflowStep) -> dict[str, object]:
    value: dict[str, object] = {"action": step.action}
    if step.selector is not None:
        value["selector"] = step.selector
    if step.value is not None:
        value["value"] = step.value
    if step.x is not None:
        value["x"] = step.x
    if step.y is not None:
        value["y"] = step.y
    if step.from_x is not None:
        value["from_x"] = step.from_x
    if step.from_y is not None:
        value["from_y"] = step.from_y
    if step.to_x is not None:
        value["to_x"] = step.to_x
    if step.to_y is not None:
        value["to_y"] = step.to_y
    if step.from_current:
        value["from_current"] = step.from_current
    if step.from_ratio_x is not None:
        value["from_ratio_x"] = step.from_ratio_x
    if step.from_ratio_y is not None:
        value["from_ratio_y"] = step.from_ratio_y
    if step.to_ratio_x is not None:
        value["to_ratio_x"] = step.to_ratio_x
    if step.to_ratio_y is not None:
        value["to_ratio_y"] = step.to_ratio_y
    if step.button is not None:
        value["button"] = step.button
    if step.duration_ms is not None:
        value["duration_ms"] = step.duration_ms
    if step.relative_x is not None:
        value["relative_x"] = step.relative_x
    if step.relative_y is not None:
        value["relative_y"] = step.relative_y
    if step.region is not None:
        value["region"] = step.region
    if step.ratio_x is not None:
        value["ratio_x"] = step.ratio_x
    if step.ratio_y is not None:
        value["ratio_y"] = step.ratio_y
    if step.window_id is not None:
        value["window_id"] = step.window_id
    if step.app is not None:
        value["app"] = step.app
    if step.window_title is not None:
        value["window_title"] = step.window_title
    if step.title_regex is not None:
        value["title_regex"] = step.title_regex
    if step.steps is not None:
        value["steps"] = step.steps
    if step.bounds_policy is not None:
        value["bounds_policy"] = step.bounds_policy
    if step.backend is not None:
        value["backend"] = step.backend
    if step.clipboard_backend is not None:
        value["clipboard_backend"] = step.clipboard_backend
    if step.hotkey_backend is not None:
        value["hotkey_backend"] = step.hotkey_backend
    if step.typing_speed_chars_per_second is not None:
        value["typing_speed_chars_per_second"] = step.typing_speed_chars_per_second
    if step.delay_ms is not None:
        value["delay_ms"] = step.delay_ms
    if step.key_delay_ms is not None:
        value["key_delay_ms"] = step.key_delay_ms
    if step.repeat is not None:
        value["repeat"] = step.repeat
    if step.interval_ms is not None:
        value["interval_ms"] = step.interval_ms
    if step.release_before:
        value["release_before"] = step.release_before
    if step.release_after:
        value["release_after"] = step.release_after
    if step.preserve_clipboard:
        value["preserve_clipboard"] = step.preserve_clipboard
    if step.restore_delay_ms is not None:
        value["restore_delay_ms"] = step.restore_delay_ms
    if step.restore_policy is not None:
        value["restore_policy"] = step.restore_policy
    if step.restore:
        value["restore"] = step.restore
    if step.dry_run:
        value["dry_run"] = step.dry_run
    if step.vision_fallback:
        value["vision_fallback"] = step.vision_fallback
    if not step.verify:
        value["verify"] = step.verify
    for key in (
        "target",
        "image_path",
        "expected_path",
        "actual_path",
        "language",
        "whitelist",
        "size_policy",
        "alpha",
        "sort",
        "mask_output_path",
        "overlay_output_path",
        "assertion",
        "expected_text",
        "plugin_id",
        "tool",
        "trust_policy",
    ):
        item = getattr(step, key)
        if item is not None:
            value[key] = item
    for key in ("image_paths", "ignore_regions", "config"):
        item = getattr(step, key)
        if item:
            value[key] = list(item)
    for key in (
        "page_segmentation_mode",
        "engine_mode",
        "dpi",
        "threshold",
        "per_channel_threshold",
        "max_changed_pixels",
        "max_channel_delta",
        "stable_max_changed_pixels",
        "stable_max_channel_delta",
        "loading_min_changed_pixels",
        "required_stable_transitions",
        "edge_threshold",
        "min_width",
        "min_height",
        "min_component_pixels",
        "max_elements",
        "merge_distance",
        "max_width",
        "max_height",
        "min_area",
        "max_area",
        "padding",
        "wait_after_focus_ms",
        "overview_wait_ms",
        "max_output_bytes",
        "sleep_ms",
    ):
        item = getattr(step, key)
        if item is not None:
            value[key] = item
    for key in (
        "min_confidence",
        "scale",
        "contrast",
        "max_changed_ratio",
        "max_mean_absolute_error",
        "stable_max_changed_ratio",
        "stable_max_mean_absolute_error",
        "loading_min_changed_ratio",
        "timeout_seconds",
    ):
        item = getattr(step, key)
        if item is not None:
            value[key] = item
    for key in (
        "grayscale",
        "invert",
        "deskew",
        "prefer_accessibility",
        "use_gnome_overview",
        "launch_if_needed",
        "clear",
    ):
        item = getattr(step, key)
        default = key in {"prefer_accessibility", "use_gnome_overview", "launch_if_needed"}
        if item != default:
            value[key] = item
    if step.arguments is not None:
        value["arguments"] = dict(step.arguments)
    if step.require_trusted is not None:
        value["require_trusted"] = step.require_trusted
    return value


def _detect_output_format(path: Path, format_name: str | None) -> str:
    if format_name is not None:
        return format_name.casefold()
    suffix = path.suffix.casefold()
    if suffix == ".json":
        return "json"
    if suffix in {".yaml", ".yml"}:
        return "yaml"
    return "json"


def _detect_format(text: str, format_name: str | None) -> str:
    if format_name is not None:
        return format_name.casefold()
    stripped = text.lstrip()
    if stripped.startswith("{"):
        return "json"
    return "yaml"


def _dump_simple_yaml(workflow: Workflow) -> str:
    validate_workflow(workflow)
    lines = [
        f"schema_version: {_format_yaml_scalar(workflow.schema_version)}",
        f"name: {_format_yaml_scalar(workflow.name)}",
        "steps:",
    ]
    for step in workflow.steps:
        step_value = workflow_step_to_dict(step)
        action = step_value.pop("action")
        lines.append(f"  - action: {_format_yaml_scalar(action)}")
        for key, value in step_value.items():
            lines.append(f"    {key}: {_format_yaml_scalar(value)}")
    return "\n".join(lines) + "\n"


def _format_yaml_scalar(value: object) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if value is None:
        return "null"
    if isinstance(value, str):
        return "'" + value.replace("'", "''") + "'"
    return "'" + str(value).replace("'", "''") + "'"


def _parse_simple_yaml(text: str) -> dict[str, object]:
    lines = _yaml_lines(text)
    data: dict[str, object] = {}
    index = 0
    while index < len(lines):
        indent, content = lines[index]
        if indent != 0:
            raise ValueError("top-level YAML keys must not be indented")
        key, raw_value = _yaml_key_value(content)
        if raw_value:
            data[key] = _parse_scalar(raw_value)
            index += 1
            continue

        if key != "steps":
            raise ValueError(f"unsupported nested YAML key: {key}")
        index += 1
        steps: list[dict[str, object]] = []
        while index < len(lines):
            item_indent, item_content = lines[index]
            if item_indent == 0:
                break
            if not item_content.startswith("-"):
                raise ValueError("workflow steps must be YAML list items")
            item: dict[str, object] = {}
            rest = item_content[1:].strip()
            if rest:
                step_key, step_value = _yaml_key_value(rest)
                item[step_key] = _parse_scalar(step_value)
            index += 1

            while index < len(lines) and lines[index][0] > item_indent:
                _child_indent, child_content = lines[index]
                child_key, child_value = _yaml_key_value(child_content)
                item[child_key] = _parse_scalar(child_value)
                index += 1
            steps.append(item)
        data[key] = steps
    return data


def _yaml_lines(text: str) -> list[tuple[int, str]]:
    lines: list[tuple[int, str]] = []
    for raw_line in text.splitlines():
        if not raw_line.strip() or raw_line.lstrip().startswith("#"):
            continue
        indent = len(raw_line) - len(raw_line.lstrip(" "))
        lines.append((indent, raw_line.strip()))
    return lines


def _yaml_key_value(content: str) -> tuple[str, str]:
    if ":" not in content:
        raise ValueError(f"YAML line must be key/value: {content}")
    key, value = content.split(":", 1)
    key = key.strip()
    if not key:
        raise ValueError("YAML key must not be empty")
    return key, value.strip()


def _parse_scalar(value: str) -> object:
    if value == "":
        return ""
    if len(value) >= 2 and value[0] == value[-1] and value[0] == "'":
        return value[1:-1].replace("''", "'")
    if len(value) >= 2 and value[0] == value[-1] and value[0] == '"':
        return value[1:-1]
    normalized = value.casefold()
    if normalized in {"true", "yes", "on"}:
        return True
    if normalized in {"false", "no", "off"}:
        return False
    if normalized in {"null", "none", "~"}:
        return None
    if value.startswith("[") or value.startswith("{"):
        try:
            return json.loads(value)
        except json.JSONDecodeError:
            pass
    if _is_int(value):
        return int(value)
    if _is_float(value):
        return float(value)
    return value


def _optional_string(value: dict[str, object], name: str) -> str | None:
    item = value.get(name)
    if item is None:
        return None
    if not isinstance(item, str):
        raise ValueError(f"{name} must be a string")
    return item


def _optional_int(value: dict[str, object], name: str) -> int | None:
    item = value.get(name)
    if item is None:
        return None
    if isinstance(item, bool) or not isinstance(item, int):
        raise ValueError(f"{name} must be an integer")
    return item


def _optional_float(value: dict[str, object], name: str) -> float | None:
    item = value.get(name)
    if item is None:
        return None
    if isinstance(item, bool) or not isinstance(item, int | float):
        raise ValueError(f"{name} must be a number")
    return float(item)


def _optional_bool(value: dict[str, object], name: str, default: bool) -> bool:
    item = value.get(name)
    if item is None:
        return default
    if isinstance(item, bool):
        return item
    if isinstance(item, str):
        normalized = item.casefold()
        if normalized in {"true", "yes", "on"}:
            return True
        if normalized in {"false", "no", "off"}:
            return False
    raise ValueError(f"{name} must be a boolean")


def _optional_bool_or_none(value: dict[str, object], name: str) -> bool | None:
    if name not in value or value.get(name) is None:
        return None
    return _optional_bool(value, name, default=False)


def _optional_string_tuple(value: dict[str, object], name: str) -> tuple[str, ...]:
    item = value.get(name)
    if item is None:
        return ()
    if isinstance(item, str):
        values = [part.strip() for part in item.split(",")]
        return tuple(part for part in values if part)
    if isinstance(item, list) and all(isinstance(part, str) for part in item):
        return tuple(part for part in item if part)
    raise ValueError(f"{name} must be a string or list of strings")


def _optional_object(value: dict[str, object], name: str) -> dict[str, object] | None:
    item = value.get(name)
    if item is None:
        return None
    if not isinstance(item, dict):
        raise ValueError(f"{name} must be an object")
    return dict(item)


def _is_int(value: str) -> bool:
    if value.startswith("-"):
        return value[1:].isdigit()
    return value.isdigit()


def _is_float(value: str) -> bool:
    try:
        float(value)
    except ValueError:
        return False
    return any(character in value for character in ".eE")
