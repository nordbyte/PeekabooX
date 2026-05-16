import re
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

from peekaboox.memory import GraphNode
from peekaboox.workflows import (
    Workflow,
    WorkflowStep,
    load_workflow_text,
    workflow_from_dict,
)


@dataclass(frozen=True, slots=True)
class WorkflowRefinementRequest:
    goal: str
    draft: Workflow
    desktop_nodes: tuple[GraphNode, ...]


WorkflowRefinementProvider = Callable[
    [WorkflowRefinementRequest],
    Workflow | dict[str, object] | str,
]


@dataclass(frozen=True, slots=True)
class WorkflowReplanningRequest:
    goal: str
    failed_workflow: Workflow
    failed_step_index: int
    reason: str
    desktop_nodes: tuple[GraphNode, ...]
    attempts: int


WorkflowReplanningProvider = Callable[
    [WorkflowReplanningRequest],
    Workflow | dict[str, object] | str,
]


@dataclass(slots=True)
class PlanningEngine:
    max_steps: int = 16
    workflow_refiner: WorkflowRefinementProvider | None = None
    workflow_replanner: WorkflowReplanningProvider | None = None

    def decompose(self, goal: str) -> list[str]:
        normalized = goal.strip()
        if not normalized:
            raise ValueError("goal must not be empty")
        return [normalized]

    def plan_workflow(self, goal: str) -> Workflow:
        normalized = goal.strip()
        if not normalized:
            raise ValueError("goal must not be empty")
        return Workflow(
            name=normalized,
            steps=[WorkflowStep(action="observe", value=normalized)],
        )

    def generate_workflow(
        self,
        goal: str,
        *,
        desktop_nodes: tuple[GraphNode, ...] | list[GraphNode] = (),
    ) -> Workflow:
        normalized = goal.strip()
        if not normalized:
            raise ValueError("goal must not be empty")

        steps: list[WorkflowStep] = [WorkflowStep(action="observe", value=normalized)]
        target = _target_selector(normalized, desktop_nodes)
        typed_text = _typed_text(normalized)
        intent = _intent(normalized)

        if target is not None and (intent in {"click", "type", "paste"} or typed_text is None):
            steps.append(WorkflowStep(action="find_element", selector=target))

        if target is not None and intent in {"click", "type", "paste"}:
            steps.append(
                WorkflowStep(action="click", selector=target, vision_fallback=True)
            )

        if typed_text is not None:
            steps.append(
                WorkflowStep(
                    action="paste_text" if intent == "paste" else "type_text",
                    value=typed_text,
                )
            )

        if len(steps) == 1 and target is not None:
            steps.append(WorkflowStep(action="find_element", selector=target))

        return self._validate_workflow(Workflow(name=normalized, steps=steps[: self.max_steps]))

    def refine_workflow(
        self,
        goal: str,
        *,
        draft: Workflow | None = None,
        desktop_nodes: tuple[GraphNode, ...] | list[GraphNode] = (),
        provider: WorkflowRefinementProvider | None = None,
    ) -> Workflow:
        normalized = goal.strip()
        if not normalized:
            raise ValueError("goal must not be empty")

        draft = draft or self.generate_workflow(
            normalized,
            desktop_nodes=desktop_nodes,
        )
        refiner = provider or self.workflow_refiner
        if refiner is None:
            return self._validate_workflow(draft)

        request = WorkflowRefinementRequest(
            goal=normalized,
            draft=draft,
            desktop_nodes=tuple(desktop_nodes),
        )
        proposed = _workflow_from_provider_result(refiner(request), default_name=normalized)
        return self._validate_workflow(proposed)

    def replan_workflow(
        self,
        goal: str,
        *,
        failed_workflow: Workflow,
        failed_step_index: int,
        reason: str,
        attempts: int = 0,
        desktop_nodes: tuple[GraphNode, ...] | list[GraphNode] = (),
        provider: WorkflowReplanningProvider | None = None,
    ) -> Workflow:
        normalized = goal.strip()
        if not normalized:
            raise ValueError("goal must not be empty")
        replanner = provider or self.workflow_replanner
        if replanner is None:
            return self.refine_workflow(
                normalized,
                draft=self.generate_workflow(normalized, desktop_nodes=desktop_nodes),
                desktop_nodes=desktop_nodes,
            )
        request = WorkflowReplanningRequest(
            goal=normalized,
            failed_workflow=failed_workflow,
            failed_step_index=failed_step_index,
            reason=reason,
            desktop_nodes=tuple(desktop_nodes),
            attempts=attempts,
        )
        proposed = _workflow_from_provider_result(replanner(request), default_name=normalized)
        return self._validate_workflow(proposed)

    def _validate_workflow(self, workflow: Workflow) -> Workflow:
        if not workflow.name.strip():
            raise ValueError("workflow name must not be empty")
        if not workflow.steps:
            raise ValueError("workflow must contain at least one step")
        if len(workflow.steps) > self.max_steps:
            raise ValueError(f"workflow exceeds max_steps={self.max_steps}")

        for index, step in enumerate(workflow.steps):
            _validate_step(step, index)
        return workflow


def _workflow_from_provider_result(value: object, default_name: str) -> Workflow:
    if isinstance(value, Workflow):
        return value
    if isinstance(value, str):
        return load_workflow_text(value)
    if isinstance(value, dict):
        workflow_value: Any = value.get("workflow", value)
        if not isinstance(workflow_value, dict):
            raise ValueError("provider workflow must be an object")
        return workflow_from_dict(workflow_value, default_name=default_name)
    raise ValueError("provider must return Workflow, workflow dict, or JSON/YAML text")


def _validate_step(step: WorkflowStep, index: int) -> None:
    action = step.action.strip().lower()
    if action not in _SUPPORTED_ACTIONS:
        raise ValueError(f"steps[{index}].action is unsupported: {step.action}")
    if action == "find_element" and not step.selector:
        raise ValueError(f"steps[{index}].selector is required for find_element")
    if action == "click":
        has_selector = bool(step.selector)
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
        if sum(1 for value in (has_selector, has_coordinates, has_scope) if value) != 1:
            raise ValueError(f"steps[{index}] click requires exactly one target")
        if has_coordinates and (step.x is None or step.y is None):
            raise ValueError(f"steps[{index}] click x/y target is incomplete")
        if step.button is not None and step.button.casefold() not in {"left", "middle", "right"}:
            raise ValueError(f"steps[{index}].button must be left, middle, or right")
        if step.bounds_policy is not None and step.bounds_policy not in {
            "allow",
            "clamp",
            "fail",
            "fail-out-of-bounds",
        }:
            raise ValueError(f"steps[{index}].bounds_policy is unsupported")
        if step.backend is not None and step.backend not in {
            "auto",
            "uinput",
            "ydotool",
            "xdotool",
        }:
            raise ValueError(f"steps[{index}].backend is unsupported")
    if action in {"move", "move_mouse"}:
        has_coordinates = step.x is not None or step.y is not None
        has_relative = step.relative_x is not None or step.relative_y is not None
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
        if sum(1 for value in (has_coordinates, has_relative, has_scope) if value) != 1:
            raise ValueError(f"steps[{index}] move_mouse requires exactly one target")
        if has_coordinates and (step.x is None or step.y is None):
            raise ValueError(f"steps[{index}] move_mouse x/y target is incomplete")
        if has_relative and (step.relative_x is None or step.relative_y is None):
            raise ValueError(f"steps[{index}] move_mouse relative target is incomplete")
        if step.duration_ms is not None and step.duration_ms < 0:
            raise ValueError(f"steps[{index}].duration_ms must be non-negative")
        if step.steps is not None and step.steps <= 0:
            raise ValueError(f"steps[{index}].steps must be greater than zero")
    if action == "drag":
        has_from_coordinates = step.from_x is not None or step.from_y is not None
        has_from_ratio = step.from_ratio_x is not None or step.from_ratio_y is not None
        from_count = sum(
            1 for value in (has_from_coordinates, step.from_current, has_from_ratio) if value
        )
        if from_count != 1:
            raise ValueError(f"steps[{index}] drag requires exactly one from endpoint")
        if has_from_coordinates and (step.from_x is None or step.from_y is None):
            raise ValueError(f"steps[{index}] drag from_x/from_y endpoint is incomplete")
        if has_from_ratio and (step.from_ratio_x is None or step.from_ratio_y is None):
            raise ValueError(f"steps[{index}] drag from_ratio_x/from_ratio_y endpoint is incomplete")

        has_to_coordinates = step.to_x is not None or step.to_y is not None
        has_to_ratio = step.to_ratio_x is not None or step.to_ratio_y is not None
        if sum(1 for value in (has_to_coordinates, has_to_ratio) if value) != 1:
            raise ValueError(f"steps[{index}] drag requires exactly one to endpoint")
        if has_to_coordinates and (step.to_x is None or step.to_y is None):
            raise ValueError(f"steps[{index}] drag to_x/to_y endpoint is incomplete")
        if has_to_ratio and (step.to_ratio_x is None or step.to_ratio_y is None):
            raise ValueError(f"steps[{index}] drag to_ratio_x/to_ratio_y endpoint is incomplete")
        if step.region is not None and not has_from_ratio and not has_to_ratio:
            raise ValueError(f"steps[{index}] drag region requires a ratio endpoint")
        if step.duration_ms is not None and step.duration_ms < 0:
            raise ValueError(f"steps[{index}].duration_ms must be non-negative")
        if step.steps is not None and step.steps <= 0:
            raise ValueError(f"steps[{index}].steps must be greater than zero")
        if step.button is not None and step.button.casefold() not in {"left", "middle", "right"}:
            raise ValueError(f"steps[{index}].button must be left, middle, or right")
    if action in {"type", "type_text", "paste", "paste_text"} and step.value is None:
        raise ValueError(f"steps[{index}].value is required for type_text")
    if action in {"type", "type_text"}:
        if (
            step.typing_speed_chars_per_second is not None
            and step.typing_speed_chars_per_second <= 0
        ):
            raise ValueError(
                f"steps[{index}].typing_speed_chars_per_second must be greater than zero"
            )
        if step.delay_ms is not None and step.delay_ms < 0:
            raise ValueError(f"steps[{index}].delay_ms must be non-negative")
        if step.key_delay_ms is not None and step.key_delay_ms < 0:
            raise ValueError(f"steps[{index}].key_delay_ms must be non-negative")
        if (
            step.typing_speed_chars_per_second is not None
            and step.key_delay_ms is not None
        ):
            raise ValueError(
                f"steps[{index}].typing_speed_chars_per_second cannot be combined with key_delay_ms"
            )
        if step.backend is not None and step.backend not in {
            "auto",
            "wtype",
            "ydotool",
            "xdotool",
        }:
            raise ValueError(f"steps[{index}].backend is unsupported")
    if action == "hotkey" and step.value is None:
        raise ValueError(f"steps[{index}].value is required for hotkey")


_SUPPORTED_ACTIONS = {
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


def _intent(goal: str) -> str | None:
    normalized = goal.casefold()
    if "paste" in normalized:
        return "paste"
    if any(word in normalized for word in ("type", "enter", "write", "input")):
        return "type"
    if any(word in normalized for word in ("click", "press", "select", "open", "submit")):
        return "click"
    return None


def _typed_text(goal: str) -> str | None:
    quoted = re.search(r'"([^"]+)"|\'([^\']+)\'', goal)
    if quoted is not None:
        return quoted.group(1) or quoted.group(2)
    typed = re.search(
        r"\b(?:type|enter|write|input|paste)\s+(.+?)"
        r"(?:\s+(?:into|in|to|and)\b|$)",
        goal,
        re.IGNORECASE,
    )
    if typed is None:
        return None
    value = typed.group(1).strip()
    return value or None


def _target_selector(
    goal: str,
    desktop_nodes: tuple[GraphNode, ...] | list[GraphNode],
) -> str | None:
    graph_target = _target_selector_from_graph(goal, desktop_nodes)
    if graph_target is not None:
        return graph_target
    label = _target_label_from_goal(goal)
    if label is None:
        return None
    return f"label={label}"


def _target_selector_from_graph(
    goal: str,
    desktop_nodes: tuple[GraphNode, ...] | list[GraphNode],
) -> str | None:
    candidates: list[tuple[int, GraphNode]] = []
    normalized_goal = goal.casefold()
    for node in desktop_nodes:
        if node.kind != "element" or node.label is None:
            continue
        label = node.label.strip()
        if not _selector_safe(label) or label.casefold() not in normalized_goal:
            continue
        score = len(label)
        if node.role and node.role.casefold() in normalized_goal:
            score += 20
        candidates.append((score, node))

    if not candidates:
        return None
    _score, node = max(candidates, key=lambda item: item[0])
    return _selector_for_node(node)


def _selector_for_node(node: GraphNode) -> str | None:
    label = node.label.strip() if node.label is not None else ""
    if not _selector_safe(label):
        return None
    parts: list[str] = []
    if node.role and _selector_safe(node.role):
        parts.append(f"role={node.role}")
    parts.append(f"label={label}")
    return ",".join(parts)


def _target_label_from_goal(goal: str) -> str | None:
    match = re.search(
        r"\b(?:click|press|select|open|submit|find)\s+(.+?)"
        r"(?:\s+(?:button|field|link|and|then|with|to|into|in)\b|$)",
        goal,
        re.IGNORECASE,
    )
    if match is None:
        return None
    label = match.group(1).strip(" .:")
    if not label or len(label.split()) > 5 or not _selector_safe(label):
        return None
    return label


def _selector_safe(value: str) -> bool:
    return bool(value.strip()) and not any(separator in value for separator in ",\n\r")
