from dataclasses import dataclass, field

from peekaboox.client import DesktopState, UiElement
from peekaboox.memory.events import (
    DesktopGraphInvalidation,
    DesktopGraphStatus,
    DesktopGraphUpdate,
    DesktopStateEvent,
)
from peekaboox.memory.graph import DesktopGraphSnapshot, GraphEdge, GraphNode, SemanticDesktopGraph
from peekaboox.memory.selectors import cached_elements_for_selector


@dataclass(slots=True)
class MemoryStore:
    values: dict[str, str] = field(default_factory=dict)
    desktop_graph: SemanticDesktopGraph = field(default_factory=SemanticDesktopGraph)
    desktop_events: list[DesktopStateEvent] = field(default_factory=list)
    desktop_graph_invalidations: list[DesktopGraphInvalidation] = field(default_factory=list)
    desktop_graph_stale: bool = False

    def put(self, key: str, value: str) -> None:
        if not key:
            raise ValueError("memory key must not be empty")
        self.values[key] = value

    def get(self, key: str) -> str | None:
        return self.values.get(key)

    def ingest_desktop_state(
        self,
        state: DesktopState,
        snapshot_id: str | None = None,
        captured_at_unix_ms: int | None = None,
    ) -> DesktopGraphSnapshot:
        snapshot = self.desktop_graph.ingest_desktop_state(
            state,
            snapshot_id=snapshot_id,
            captured_at_unix_ms=captured_at_unix_ms,
        )
        self.desktop_graph_stale = False
        return snapshot

    def latest_desktop_snapshot(self) -> DesktopGraphSnapshot | None:
        return self.desktop_graph.latest_snapshot()

    def record_desktop_event(
        self,
        *,
        kind: str,
        source: str = "runtime",
        target_id: str | None = None,
        payload: dict[str, object] | None = None,
        occurred_at_unix_ms: int | None = None,
        state: DesktopState | None = None,
        snapshot_id: str | None = None,
    ) -> DesktopGraphUpdate:
        event = DesktopStateEvent.create(
            kind=kind,
            source=source,
            occurred_at_unix_ms=occurred_at_unix_ms,
            target_id=target_id,
            payload=payload,
        )
        self.desktop_events.append(event)
        if state is not None:
            snapshot = self.ingest_desktop_state(
                state,
                snapshot_id=snapshot_id,
                captured_at_unix_ms=event.occurred_at_unix_ms,
            )
            return DesktopGraphUpdate(event=event, snapshot=snapshot, stale=False)

        invalidation = self.invalidate_desktop_graph(event)
        return DesktopGraphUpdate(event=event, invalidation=invalidation, stale=True)

    def invalidate_desktop_graph(
        self,
        event: DesktopStateEvent,
        reason: str | None = None,
    ) -> DesktopGraphInvalidation:
        snapshot = self.latest_desktop_snapshot()
        invalidation = DesktopGraphInvalidation(
            event=event,
            invalidated_snapshot_id=snapshot.id if snapshot else None,
            affected_node_ids=_affected_node_ids(snapshot, event),
            reason=reason or _invalidation_reason(event),
            requires_refresh=True,
        )
        self.desktop_graph_invalidations.append(invalidation)
        self.desktop_graph_stale = True
        return invalidation

    def desktop_graph_status(self) -> DesktopGraphStatus:
        latest = self.latest_desktop_snapshot()
        stats = self.desktop_graph.stats()
        return DesktopGraphStatus(
            stale=self.desktop_graph_stale,
            latest_snapshot_id=latest.id if latest else None,
            event_count=len(self.desktop_events),
            invalidation_count=len(self.desktop_graph_invalidations),
            snapshot_count=stats["snapshot_count"],
            node_count=stats["node_count"],
            edge_count=stats["edge_count"],
            last_event=self.desktop_events[-1] if self.desktop_events else None,
            last_invalidation=(
                self.desktop_graph_invalidations[-1]
                if self.desktop_graph_invalidations
                else None
            ),
        )

    def compact_desktop_graph(
        self,
        *,
        max_snapshots: int | None = None,
        max_age_ms: int | None = None,
        now_unix_ms: int | None = None,
    ) -> int:
        return self.desktop_graph.compact(
            max_snapshots=max_snapshots,
            max_age_ms=max_age_ms,
            now_unix_ms=now_unix_ms,
        )

    def query_desktop_nodes(
        self,
        *,
        kind: str | None = None,
        label_contains: str | None = None,
        role: str | None = None,
        attribute_equals: dict[str, object] | None = None,
        contained_by: str | None = None,
        latest_only: bool = True,
    ) -> tuple[GraphNode, ...]:
        return self.desktop_graph.find_nodes(
            kind=kind,
            label_contains=label_contains,
            role=role,
            attribute_equals=attribute_equals,
            contained_by=contained_by,
            latest_only=latest_only,
        )

    def query_desktop_edges(
        self,
        *,
        source: str | None = None,
        target: str | None = None,
        kind: str | None = None,
        latest_only: bool = True,
    ) -> tuple[GraphEdge, ...]:
        return self.desktop_graph.query_edges(
            source=source,
            target=target,
            kind=kind,
            latest_only=latest_only,
        )

    def find_cached_elements(
        self,
        selector: str,
        *,
        latest_only: bool = True,
    ) -> tuple[UiElement, ...]:
        if self.desktop_graph_stale:
            return ()
        snapshot = self.latest_desktop_snapshot()
        if snapshot is None:
            return ()
        try:
            nodes = self.desktop_graph.find_nodes(kind="element", latest_only=latest_only)
            return cached_elements_for_selector(nodes, selector)
        except ValueError:
            return ()

    def export_desktop_graph(self) -> str:
        return self.desktop_graph.to_json()

    def import_desktop_graph(self, value: str) -> None:
        self.desktop_graph = SemanticDesktopGraph.from_json(value)
        self.desktop_graph_stale = False


def _affected_node_ids(
    snapshot: DesktopGraphSnapshot | None,
    event: DesktopStateEvent,
) -> tuple[str, ...]:
    if snapshot is None:
        return ()

    node_ids = {node.id for node in snapshot.nodes}
    target_ids = _candidate_target_ids(event)
    if target_ids:
        affected = {node_id for node_id in target_ids if node_id in node_ids}
        for node_id in tuple(affected):
            affected.update(_contained_node_ids(snapshot, node_id))
        return tuple(node.id for node in snapshot.nodes if node.id in affected)

    event_kind = event.kind.casefold()
    if "focus" in event_kind:
        return tuple(node.id for node in snapshot.nodes if node.kind == "window")
    return tuple(node.id for node in snapshot.nodes)


def _candidate_target_ids(event: DesktopStateEvent) -> tuple[str, ...]:
    if event.target_id is None:
        return ()
    target_id = event.target_id
    if target_id.startswith(("window:", "element:", "snapshot:")):
        return (target_id,)
    event_kind = event.kind.casefold()
    candidates = [target_id]
    if "window" in event_kind or "focus" in event_kind:
        candidates.append(f"window:{target_id}")
    if "element" in event_kind or "accessibility" in event_kind or "object" in event_kind:
        candidates.append(f"element:{target_id}")
    if len(candidates) == 1:
        candidates.extend((f"window:{target_id}", f"element:{target_id}"))
    return tuple(dict.fromkeys(candidates))


def _contained_node_ids(snapshot: DesktopGraphSnapshot, node_id: str) -> tuple[str, ...]:
    return tuple(
        edge.target
        for edge in snapshot.edges
        if edge.source == node_id and edge.kind == "contains"
    )


def _invalidation_reason(event: DesktopStateEvent) -> str:
    return f"{event.source}:{event.kind}"
