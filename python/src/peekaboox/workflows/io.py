from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from peekaboox.workflows.model import Workflow, WorkflowStep


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
    name = value.get("name", default_name)
    if not isinstance(name, str) or not name.strip():
        raise ValueError("workflow name must be a non-empty string")
    steps = value.get("steps")
    if not isinstance(steps, list) or not steps:
        raise ValueError("workflow steps must be a non-empty list")
    return Workflow(
        name=name,
        steps=[workflow_step_from_dict(step, index) for index, step in enumerate(steps)],
    )


def workflow_to_dict(workflow: Workflow) -> dict[str, object]:
    return {
        "name": workflow.name,
        "steps": [workflow_step_to_dict(step) for step in workflow.steps],
    }


def workflow_step_from_dict(value: Any, index: int = 0) -> WorkflowStep:
    if not isinstance(value, dict):
        raise ValueError(f"steps[{index}] must be an object")
    action = value.get("action")
    if not isinstance(action, str) or not action.strip():
        raise ValueError(f"steps[{index}].action must be a non-empty string")
    return WorkflowStep(
        action=action,
        selector=_optional_string(value, "selector"),
        value=_optional_string(value, "value"),
        x=_optional_int(value, "x"),
        y=_optional_int(value, "y"),
        from_x=_optional_int(value, "from_x"),
        from_y=_optional_int(value, "from_y"),
        to_x=_optional_int(value, "to_x"),
        to_y=_optional_int(value, "to_y"),
        button=_optional_string(value, "button"),
        duration_ms=_optional_int(value, "duration_ms"),
        vision_fallback=_optional_bool(value, "vision_fallback", default=False),
        verify=_optional_bool(value, "verify", default=True),
    )


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
    if step.button is not None:
        value["button"] = step.button
    if step.duration_ms is not None:
        value["duration_ms"] = step.duration_ms
    if step.vision_fallback:
        value["vision_fallback"] = step.vision_fallback
    if not step.verify:
        value["verify"] = step.verify
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
    lines = [f"name: {_format_yaml_scalar(workflow.name)}", "steps:"]
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
    if _is_int(value):
        return int(value)
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


def _is_int(value: str) -> bool:
    if value.startswith("-"):
        return value[1:].isdigit()
    return value.isdigit()
