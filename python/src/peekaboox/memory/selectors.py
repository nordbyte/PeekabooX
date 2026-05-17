from __future__ import annotations

from dataclasses import dataclass

from peekaboox.client import Point, Rect, UiElement
from peekaboox.memory.graph import GraphNode


@dataclass(frozen=True, slots=True)
class SemanticSelector:
    role: str | None = None
    label: str | None = None
    bounds: Rect | None = None
    contains_point: tuple[int, int] | None = None
    state: str | None = None
    min_confidence: float | None = None

    @classmethod
    def parse(cls, selector: str) -> SemanticSelector:
        selector = selector.strip()
        if not selector:
            raise ValueError("semantic selector must not be empty")

        role: str | None = None
        label: str | None = None
        bounds: Rect | None = None
        contains_point: tuple[int, int] | None = None
        state: str | None = None
        min_confidence: float | None = None

        for part in _split_selector_parts(selector):
            parsed = _selector_key_value(part)
            if parsed is None:
                if label is None:
                    label = _non_empty(part)
                continue

            key, value = parsed
            normalized_key = key.strip().casefold()
            value = value.strip()
            if normalized_key == "role":
                role = _non_empty(value)
            elif normalized_key in {"label", "name", "text"}:
                label = _non_empty(value)
            elif normalized_key in {"bounds", "rect"}:
                bounds = _parse_rect(value)
                if bounds is None:
                    raise ValueError("bounds selector must have x,y,width,height")
            elif normalized_key in {"contains", "point", "at"}:
                contains_point = _parse_point(value)
                if contains_point is None:
                    raise ValueError("contains selector must have x,y")
            elif normalized_key in {"state", "states"}:
                state = _non_empty(value)
            elif normalized_key in {"confidence", "confidence>", "min_confidence", "min-confidence"}:
                min_confidence = float(value)
            elif label is None:
                label = _non_empty(part)

        return cls(
            role=role,
            label=label,
            bounds=bounds,
            contains_point=contains_point,
            state=state,
            min_confidence=min_confidence,
        )

    def matches(self, element: UiElement) -> bool:
        return (
            _matches_optional_contains(element.role, self.role)
            and _matches_optional_contains(element.label, self.label)
            and (self.bounds is None or element.bounds == self.bounds)
            and (
                self.contains_point is None
                or _rect_contains_point(element.bounds, self.contains_point)
            )
            and (
                self.state is None
                or any(
                    _contains_case_insensitive(element_state, self.state)
                    for element_state in element.states
                )
            )
            and (
                self.min_confidence is None
                or element.confidence >= self.min_confidence
            )
        )


def cached_elements_for_selector(
    nodes: tuple[GraphNode, ...] | list[GraphNode],
    selector: str,
) -> tuple[UiElement, ...]:
    parsed = SemanticSelector.parse(selector)
    elements = tuple(
        element
        for node in nodes
        if node.kind == "element"
        if (element := _element_from_node(node)) is not None
    )
    return tuple(element for element in elements if parsed.matches(element))


def _element_from_node(node: GraphNode) -> UiElement | None:
    if node.bounds is None:
        return None
    element_id = node.attributes.get("element_id")
    confidence = node.attributes.get("confidence", 1.0)
    states = node.attributes.get("states", ())
    if not isinstance(states, list | tuple):
        states = ()
    child_ids = node.attributes.get("child_ids", ())
    if not isinstance(child_ids, list | tuple):
        child_ids = ()
    return UiElement(
        id=str(element_id) if element_id is not None else _strip_prefix(node.id, "element:"),
        role=node.role or "",
        label=node.label,
        bounds=node.bounds,
        confidence=float(confidence),
        center=_optional_point_attribute(node, "center"),
        states=tuple(str(state) for state in states),
        window_id=_optional_str_attribute(node, "window_id"),
        window_title=_optional_str_attribute(node, "window_title"),
        app_id=_optional_str_attribute(node, "app_id"),
        parent_id=_optional_str_attribute(node, "parent_id"),
        child_ids=tuple(str(child_id) for child_id in child_ids),
    )


def _optional_str_attribute(node: GraphNode, key: str) -> str | None:
    value = node.attributes.get(key)
    return str(value) if value is not None else None


def _optional_point_attribute(node: GraphNode, key: str) -> Point | None:
    value = node.attributes.get(key)
    if isinstance(value, Point):
        return value
    if isinstance(value, dict):
        try:
            return Point(x=int(value["x"]), y=int(value["y"]))
        except (KeyError, TypeError, ValueError):
            return None
    return None


def _split_selector_parts(selector: str) -> list[str]:
    parts = [part.strip() for part in selector.split(",") if part.strip()]
    normalized: list[str] = []
    index = 0
    while index < len(parts):
        part = parts[index]
        parsed = _selector_key_value(part)
        if parsed is not None:
            key, value = parsed
            expected_len = _numeric_selector_len(key)
            if expected_len is not None:
                value_len = _numeric_part_count(value)
                while value_len < expected_len and index + 1 < len(parts):
                    next_part = parts[index + 1]
                    if _selector_key_value(next_part) is not None:
                        break
                    part = f"{part},{next_part}"
                    index += 1
                    next_parsed = _selector_key_value(part)
                    value_len = (
                        _numeric_part_count(next_parsed[1])
                        if next_parsed is not None
                        else value_len
                    )

        normalized.append(part)
        index += 1
    return normalized


def _selector_key_value(part: str) -> tuple[str, str] | None:
    for separator in (">=", "=", ":"):
        if separator in part:
            key, value = part.split(separator, 1)
            return key, value
    return None


def _numeric_selector_len(key: str) -> int | None:
    normalized = key.strip().casefold()
    if normalized in {"bounds", "rect"}:
        return 4
    if normalized in {"contains", "point", "at"}:
        return 2
    return None


def _numeric_part_count(value: str) -> int:
    return len([part for part in value.split(",") if part.strip()])


def _parse_rect(value: str) -> Rect | None:
    parts = _parse_int_parts(value, 4)
    if parts is None:
        return None
    return Rect(x=parts[0], y=parts[1], width=parts[2], height=parts[3])


def _parse_point(value: str) -> tuple[int, int] | None:
    parts = _parse_int_parts(value, 2)
    if parts is None:
        return None
    return parts[0], parts[1]


def _parse_int_parts(value: str, expected_len: int) -> tuple[int, ...] | None:
    parts = [part.strip() for part in value.split(",") if part.strip()]
    if len(parts) != expected_len:
        return None
    try:
        return tuple(int(part) for part in parts)
    except ValueError:
        return None


def _non_empty(value: str) -> str | None:
    value = value.strip()
    return value or None


def _matches_optional_contains(value: str | None, expected: str | None) -> bool:
    if expected is None:
        return True
    return value is not None and _contains_case_insensitive(value, expected)


def _contains_case_insensitive(value: str, expected: str) -> bool:
    return expected.casefold() in value.casefold()


def _rect_contains_point(rect: Rect, point: tuple[int, int]) -> bool:
    x, y = point
    return (
        x >= rect.x
        and y >= rect.y
        and x < rect.x + rect.width
        and y < rect.y + rect.height
    )


def _strip_prefix(value: str, prefix: str) -> str:
    return value.removeprefix(prefix)
