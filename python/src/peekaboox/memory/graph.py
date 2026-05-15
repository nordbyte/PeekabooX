from __future__ import annotations

import json
import time
from collections.abc import Iterable
from dataclasses import dataclass, field
from typing import Any

from peekaboox.client import DesktopState, Rect, UiElement, WindowInfo


@dataclass(frozen=True, slots=True)
class GraphNode:
    id: str
    kind: str
    label: str | None = None
    role: str | None = None
    bounds: Rect | None = None
    attributes: dict[str, object] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class GraphEdge:
    source: str
    target: str
    kind: str
    attributes: dict[str, object] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class DesktopGraphSnapshot:
    id: str
    captured_at_unix_ms: int
    active_window_id: str | None
    nodes: tuple[GraphNode, ...]
    edges: tuple[GraphEdge, ...]

    def to_dict(self) -> dict[str, object]:
        return _snapshot_to_dict(self)

    @classmethod
    def from_dict(cls, value: dict[str, object]) -> "DesktopGraphSnapshot":
        return _snapshot_from_dict(value)

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), sort_keys=True, separators=(",", ":"))

    @classmethod
    def from_json(cls, value: str) -> "DesktopGraphSnapshot":
        decoded = json.loads(value)
        if not isinstance(decoded, dict):
            raise ValueError("desktop graph snapshot JSON must decode to an object")
        return cls.from_dict(decoded)


@dataclass(frozen=True, slots=True)
class GraphQuery:
    kind: str | None = None
    label_contains: str | None = None
    role: str | None = None
    attribute_equals: dict[str, object] = field(default_factory=dict)
    contained_by: str | None = None
    latest_only: bool = True


@dataclass(slots=True)
class SemanticDesktopGraph:
    snapshots: list[DesktopGraphSnapshot] = field(default_factory=list)

    def ingest_desktop_state(
        self,
        state: DesktopState,
        snapshot_id: str | None = None,
        captured_at_unix_ms: int | None = None,
    ) -> DesktopGraphSnapshot:
        timestamp = captured_at_unix_ms if captured_at_unix_ms is not None else _unix_ms()
        snapshot_id = snapshot_id or f"snapshot:{timestamp}:{len(self.snapshots)}"
        windows = _state_windows(state)
        snapshot_node = GraphNode(
            id=snapshot_id,
            kind="snapshot",
            label="Desktop Snapshot",
            attributes={"captured_at_unix_ms": timestamp},
        )
        active_window_id = _window_node_id(state.active_window) if state.active_window else None
        window_nodes = tuple(_window_node(window) for window in windows)
        element_nodes = _dedupe_nodes(_element_node(element) for element in state.elements)

        nodes = (snapshot_node, *window_nodes, *element_nodes)
        edges: list[GraphEdge] = []
        for window in windows:
            window_id = _window_node_id(window)
            edges.append(GraphEdge(source=snapshot_id, target=window_id, kind="has_window"))
            if window.focused:
                edges.append(GraphEdge(source=snapshot_id, target=window_id, kind="focused_window"))
            if active_window_id == window_id:
                edges.append(GraphEdge(source=snapshot_id, target=window_id, kind="active_window"))

        for element in state.elements:
            element_id = _element_node_id(element)
            edges.append(GraphEdge(source=snapshot_id, target=element_id, kind="has_element"))
            if element.parent_id:
                edges.append(
                    GraphEdge(
                        source=_normalize_element_node_id(element.parent_id),
                        target=element_id,
                        kind="parent_of",
                    )
                )
            for window in windows:
                if element.window_id == window.id or _rect_contains(window.bounds, element.bounds):
                    edges.append(
                        GraphEdge(
                            source=_window_node_id(window),
                            target=element_id,
                            kind="contains",
                        )
                    )

        snapshot = DesktopGraphSnapshot(
            id=snapshot_id,
            captured_at_unix_ms=timestamp,
            active_window_id=active_window_id,
            nodes=nodes,
            edges=tuple(edges),
        )
        self.snapshots.append(snapshot)
        return snapshot

    def latest_snapshot(self) -> DesktopGraphSnapshot | None:
        return self.snapshots[-1] if self.snapshots else None

    def node_by_id(self, node_id: str, *, latest_only: bool = True) -> GraphNode | None:
        snapshots = self._snapshots_for_query(latest_only)
        if not latest_only:
            snapshots = tuple(reversed(snapshots))
        for snapshot in snapshots:
            for node in snapshot.nodes:
                if node.id == node_id:
                    return node
        return None

    def query_nodes(self, query: GraphQuery) -> tuple[GraphNode, ...]:
        matches: list[GraphNode] = []
        for snapshot in self._snapshots_for_query(query.latest_only):
            for node in snapshot.nodes:
                if _node_matches_query(snapshot, node, query):
                    matches.append(node)
        return tuple(matches)

    def find_nodes(
        self,
        *,
        kind: str | None = None,
        label_contains: str | None = None,
        role: str | None = None,
        attribute_equals: dict[str, object] | None = None,
        contained_by: str | None = None,
        latest_only: bool = True,
    ) -> tuple[GraphNode, ...]:
        return self.query_nodes(
            GraphQuery(
                kind=kind,
                label_contains=label_contains,
                role=role,
                attribute_equals=attribute_equals or {},
                contained_by=contained_by,
                latest_only=latest_only,
            )
        )

    def query_edges(
        self,
        *,
        source: str | None = None,
        target: str | None = None,
        kind: str | None = None,
        latest_only: bool = True,
    ) -> tuple[GraphEdge, ...]:
        matches: list[GraphEdge] = []
        for snapshot in self._snapshots_for_query(latest_only):
            for edge in snapshot.edges:
                if source is not None and edge.source != source:
                    continue
                if target is not None and edge.target != target:
                    continue
                if kind is not None and edge.kind != kind:
                    continue
                matches.append(edge)
        return tuple(matches)

    def edges_for(
        self,
        node_id: str,
        *,
        kind: str | None = None,
        latest_only: bool = True,
    ) -> tuple[GraphEdge, ...]:
        matches: list[GraphEdge] = []
        for snapshot in self._snapshots_for_query(latest_only):
            for edge in snapshot.edges:
                if edge.source != node_id and edge.target != node_id:
                    continue
                if kind is not None and edge.kind != kind:
                    continue
                matches.append(edge)
        return tuple(matches)

    def _snapshots_for_query(self, latest_only: bool) -> tuple[DesktopGraphSnapshot, ...]:
        return tuple(self.snapshots[-1:]) if latest_only else tuple(self.snapshots)

    def to_dict(self) -> dict[str, object]:
        return {
            "snapshots": [_snapshot_to_dict(snapshot) for snapshot in self.snapshots],
        }

    @classmethod
    def from_dict(cls, value: dict[str, object]) -> "SemanticDesktopGraph":
        snapshots = value.get("snapshots", [])
        if not isinstance(snapshots, list):
            raise ValueError("graph snapshots must be a list")
        return cls(snapshots=[_snapshot_from_dict(snapshot) for snapshot in snapshots])

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), sort_keys=True, separators=(",", ":"))

    @classmethod
    def from_json(cls, value: str) -> "SemanticDesktopGraph":
        decoded = json.loads(value)
        if not isinstance(decoded, dict):
            raise ValueError("desktop graph JSON must decode to an object")
        return cls.from_dict(decoded)


def _window_node(window: WindowInfo) -> GraphNode:
    return GraphNode(
        id=_window_node_id(window),
        kind="window",
        label=window.title,
        role="window",
        bounds=window.bounds,
        attributes={
            "window_id": window.id,
            "app_id": window.app_id,
            "focused": window.focused,
            "state": window.state,
        },
    )


def _element_node(element: UiElement) -> GraphNode:
    return GraphNode(
        id=_element_node_id(element),
        kind="element",
        label=element.label,
        role=element.role,
        bounds=element.bounds,
        attributes={
            "element_id": element.id,
            "confidence": element.confidence,
            "states": list(element.states),
            "center": (
                {"x": element.center.x, "y": element.center.y}
                if element.center is not None
                else None
            ),
            "window_id": element.window_id,
            "window_title": element.window_title,
            "app_id": element.app_id,
            "parent_id": element.parent_id,
            "child_ids": list(element.child_ids),
        },
    )


def _state_windows(state: DesktopState) -> tuple[WindowInfo, ...]:
    windows_by_id = {window.id: window for window in state.windows}
    if state.active_window is not None and all(
        window.id != state.active_window.id for window in state.windows
    ):
        return (state.active_window, *windows_by_id.values())
    return tuple(windows_by_id.values())


def _dedupe_nodes(nodes: Iterable[GraphNode]) -> tuple[GraphNode, ...]:
    unique: dict[str, GraphNode] = {}
    for node in nodes:
        unique.setdefault(node.id, node)
    return tuple(unique.values())


def _node_matches_query(
    snapshot: DesktopGraphSnapshot,
    node: GraphNode,
    query: GraphQuery,
) -> bool:
    if query.kind is not None and node.kind != query.kind:
        return False
    if query.role is not None and node.role != query.role:
        return False
    if query.label_contains is not None and (
        query.label_contains.casefold() not in (node.label or "").casefold()
    ):
        return False
    for key, expected in query.attribute_equals.items():
        if node.attributes.get(key) != expected:
            return False
    if query.contained_by is not None and not _snapshot_contains_node(
        snapshot,
        _normalize_window_node_id(query.contained_by),
        node.id,
    ):
        return False
    return True


def _snapshot_contains_node(
    snapshot: DesktopGraphSnapshot,
    window_id: str,
    node_id: str,
) -> bool:
    return any(
        edge.source == window_id and edge.target == node_id and edge.kind == "contains"
        for edge in snapshot.edges
    )


def _normalize_window_node_id(window_id: str) -> str:
    return window_id if window_id.startswith("window:") else f"window:{window_id}"


def _normalize_element_node_id(element_id: str) -> str:
    return element_id if element_id.startswith("element:") else f"element:{element_id}"


def _window_node_id(window: WindowInfo) -> str:
    return f"window:{window.id}"


def _element_node_id(element: UiElement) -> str:
    return f"element:{element.id}"


def _rect_contains(outer: Rect, inner: Rect) -> bool:
    outer_right = outer.x + outer.width
    outer_bottom = outer.y + outer.height
    inner_right = inner.x + inner.width
    inner_bottom = inner.y + inner.height
    return (
        inner.x >= outer.x
        and inner.y >= outer.y
        and inner_right <= outer_right
        and inner_bottom <= outer_bottom
    )


def _unix_ms() -> int:
    return int(time.time() * 1000)


def _snapshot_to_dict(snapshot: DesktopGraphSnapshot) -> dict[str, object]:
    return {
        "id": snapshot.id,
        "captured_at_unix_ms": snapshot.captured_at_unix_ms,
        "active_window_id": snapshot.active_window_id,
        "nodes": [_node_to_dict(node) for node in snapshot.nodes],
        "edges": [_edge_to_dict(edge) for edge in snapshot.edges],
    }


def _snapshot_from_dict(value: Any) -> DesktopGraphSnapshot:
    if not isinstance(value, dict):
        raise ValueError("snapshot must be an object")
    return DesktopGraphSnapshot(
        id=str(value["id"]),
        captured_at_unix_ms=int(value["captured_at_unix_ms"]),
        active_window_id=(
            str(value["active_window_id"]) if value.get("active_window_id") is not None else None
        ),
        nodes=tuple(_node_from_dict(node) for node in _required_list(value, "nodes")),
        edges=tuple(_edge_from_dict(edge) for edge in _required_list(value, "edges")),
    )


def _node_to_dict(node: GraphNode) -> dict[str, object]:
    return {
        "id": node.id,
        "kind": node.kind,
        "label": node.label,
        "role": node.role,
        "bounds": _rect_to_dict(node.bounds),
        "attributes": node.attributes,
    }


def _node_from_dict(value: Any) -> GraphNode:
    if not isinstance(value, dict):
        raise ValueError("node must be an object")
    return GraphNode(
        id=str(value["id"]),
        kind=str(value["kind"]),
        label=str(value["label"]) if value.get("label") is not None else None,
        role=str(value["role"]) if value.get("role") is not None else None,
        bounds=_rect_from_dict(value.get("bounds")),
        attributes=_object_dict(value.get("attributes", {})),
    )


def _edge_to_dict(edge: GraphEdge) -> dict[str, object]:
    return {
        "source": edge.source,
        "target": edge.target,
        "kind": edge.kind,
        "attributes": edge.attributes,
    }


def _edge_from_dict(value: Any) -> GraphEdge:
    if not isinstance(value, dict):
        raise ValueError("edge must be an object")
    return GraphEdge(
        source=str(value["source"]),
        target=str(value["target"]),
        kind=str(value["kind"]),
        attributes=_object_dict(value.get("attributes", {})),
    )


def _rect_to_dict(rect: Rect | None) -> dict[str, int] | None:
    if rect is None:
        return None
    return {"x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height}


def _rect_from_dict(value: Any) -> Rect | None:
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ValueError("rect must be an object")
    return Rect(
        x=int(value["x"]),
        y=int(value["y"]),
        width=int(value["width"]),
        height=int(value["height"]),
    )


def _required_list(value: dict[str, object], name: str) -> list[object]:
    item = value.get(name)
    if not isinstance(item, list):
        raise ValueError(f"{name} must be a list")
    return item


def _object_dict(value: Any) -> dict[str, object]:
    if not isinstance(value, dict):
        raise ValueError("attributes must be an object")
    return dict(value)
