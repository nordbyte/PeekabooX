from __future__ import annotations

import json
import sqlite3
import time
from pathlib import Path
from types import TracebackType

from peekaboox.client import DesktopState
from peekaboox.memory.events import (
    DesktopGraphInvalidation,
    DesktopGraphUpdate,
    DesktopStateEvent,
)
from peekaboox.memory.graph import DesktopGraphSnapshot, GraphEdge, GraphNode, SemanticDesktopGraph
from peekaboox.memory.store import MemoryStore


class SQLiteMemoryStore(MemoryStore):
    """MemoryStore variant backed by a local SQLite database."""

    __slots__ = ("database_path", "_connection")

    def __init__(self, database_path: str | Path) -> None:
        super().__init__()
        self.database_path = (
            ":memory:" if str(database_path) == ":memory:" else str(Path(database_path).expanduser())
        )
        if self.database_path != ":memory:":
            Path(self.database_path).parent.mkdir(parents=True, exist_ok=True)
        self._connection = sqlite3.connect(self.database_path)
        self._connection.row_factory = sqlite3.Row
        self._initialize_schema()
        self._load_from_database()

    def close(self) -> None:
        self._connection.commit()
        self._connection.close()

    def __enter__(self) -> SQLiteMemoryStore:
        return self

    def __exit__(
        self,
        _exc_type: type[BaseException] | None,
        _exc: BaseException | None,
        _traceback: TracebackType | None,
    ) -> None:
        self.close()

    def put(self, key: str, value: str) -> None:
        super().put(key, value)
        with self._connection:
            self._persist_value(key, value)

    def ingest_desktop_state(
        self,
        state: DesktopState,
        snapshot_id: str | None = None,
        captured_at_unix_ms: int | None = None,
    ) -> DesktopGraphSnapshot:
        snapshot = super().ingest_desktop_state(
            state,
            snapshot_id=snapshot_id,
            captured_at_unix_ms=captured_at_unix_ms,
        )
        with self._connection:
            self._persist_snapshot(snapshot)
            self._persist_metadata("desktop_graph_stale", self.desktop_graph_stale)
        return snapshot

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
        update = super().record_desktop_event(
            kind=kind,
            source=source,
            target_id=target_id,
            payload=payload,
            occurred_at_unix_ms=occurred_at_unix_ms,
            state=state,
            snapshot_id=snapshot_id,
        )
        with self._connection:
            self._persist_event(update.event)
            if update.invalidation is not None:
                self._persist_invalidation(update.invalidation)
            if update.snapshot is not None:
                self._persist_snapshot(update.snapshot)
            self._persist_metadata("desktop_graph_stale", self.desktop_graph_stale)
        return update

    def import_desktop_graph(self, value: str) -> None:
        super().import_desktop_graph(value)
        with self._connection:
            self._connection.execute("DELETE FROM desktop_graph_snapshots")
            for snapshot in self.desktop_graph.snapshots:
                self._persist_snapshot(snapshot)
            self._persist_metadata("desktop_graph_stale", self.desktop_graph_stale)

    def flush(self) -> None:
        with self._connection:
            for key, value in self.values.items():
                self._persist_value(key, value)
            self._connection.execute("DELETE FROM desktop_state_events")
            self._connection.execute("DELETE FROM desktop_graph_invalidations")
            for snapshot in self.desktop_graph.snapshots:
                self._persist_snapshot(snapshot)
            for event in self.desktop_events:
                self._persist_event(event)
            for invalidation in self.desktop_graph_invalidations:
                self._persist_invalidation(invalidation)
            self._persist_metadata("desktop_graph_stale", self.desktop_graph_stale)

    def _initialize_schema(self) -> None:
        with self._connection:
            self._connection.execute("PRAGMA foreign_keys = ON")
            self._connection.execute(
                """
                CREATE TABLE IF NOT EXISTS memory_values (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL
                )
                """
            )
            self._connection.execute(
                """
                CREATE TABLE IF NOT EXISTS memory_metadata (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                )
                """
            )
            self._connection.execute(
                """
                CREATE TABLE IF NOT EXISTS desktop_graph_snapshots (
                    id TEXT PRIMARY KEY,
                    captured_at_unix_ms INTEGER NOT NULL,
                    active_window_id TEXT,
                    snapshot_json TEXT NOT NULL
                )
                """
            )
            self._connection.execute(
                """
                CREATE TABLE IF NOT EXISTS desktop_graph_nodes (
                    snapshot_id TEXT NOT NULL,
                    node_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    label TEXT,
                    role TEXT,
                    x INTEGER,
                    y INTEGER,
                    width INTEGER,
                    height INTEGER,
                    attributes_json TEXT NOT NULL,
                    PRIMARY KEY (snapshot_id, node_id),
                    FOREIGN KEY (snapshot_id)
                        REFERENCES desktop_graph_snapshots(id)
                        ON DELETE CASCADE
                )
                """
            )
            self._connection.execute(
                """
                CREATE TABLE IF NOT EXISTS desktop_graph_edges (
                    snapshot_id TEXT NOT NULL,
                    edge_index INTEGER NOT NULL,
                    source TEXT NOT NULL,
                    target TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    attributes_json TEXT NOT NULL,
                    PRIMARY KEY (snapshot_id, edge_index),
                    FOREIGN KEY (snapshot_id)
                        REFERENCES desktop_graph_snapshots(id)
                        ON DELETE CASCADE
                )
                """
            )
            self._connection.execute(
                """
                CREATE TABLE IF NOT EXISTS desktop_state_events (
                    event_index INTEGER PRIMARY KEY AUTOINCREMENT,
                    occurred_at_unix_ms INTEGER NOT NULL,
                    source TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    target_id TEXT,
                    event_json TEXT NOT NULL
                )
                """
            )
            self._connection.execute(
                """
                CREATE TABLE IF NOT EXISTS desktop_graph_invalidations (
                    invalidation_index INTEGER PRIMARY KEY AUTOINCREMENT,
                    occurred_at_unix_ms INTEGER NOT NULL,
                    invalidated_snapshot_id TEXT,
                    reason TEXT NOT NULL,
                    invalidation_json TEXT NOT NULL
                )
                """
            )
            self._connection.execute(
                """
                CREATE INDEX IF NOT EXISTS idx_desktop_graph_nodes_lookup
                ON desktop_graph_nodes(kind, role, label)
                """
            )
            self._connection.execute(
                """
                CREATE INDEX IF NOT EXISTS idx_desktop_graph_edges_lookup
                ON desktop_graph_edges(source, target, kind)
                """
            )
            self._connection.execute(
                """
                CREATE INDEX IF NOT EXISTS idx_desktop_state_events_lookup
                ON desktop_state_events(source, kind, occurred_at_unix_ms)
                """
            )

    def _load_from_database(self) -> None:
        self.values.update(
            {
                str(row["key"]): str(row["value"])
                for row in self._connection.execute(
                    "SELECT key, value FROM memory_values ORDER BY key"
                )
            }
        )
        self.desktop_graph = SemanticDesktopGraph(
            snapshots=[
                DesktopGraphSnapshot.from_json(str(row["snapshot_json"]))
                for row in self._connection.execute(
                    """
                    SELECT snapshot_json
                    FROM desktop_graph_snapshots
                    ORDER BY captured_at_unix_ms, rowid
                    """
                )
            ]
        )
        self.desktop_events = [
            DesktopStateEvent.from_json(str(row["event_json"]))
            for row in self._connection.execute(
                """
                SELECT event_json
                FROM desktop_state_events
                ORDER BY event_index
                """
            )
        ]
        self.desktop_graph_invalidations = [
            DesktopGraphInvalidation.from_json(str(row["invalidation_json"]))
            for row in self._connection.execute(
                """
                SELECT invalidation_json
                FROM desktop_graph_invalidations
                ORDER BY invalidation_index
                """
            )
        ]
        self.desktop_graph_stale = self._metadata_bool("desktop_graph_stale")

    def _persist_value(self, key: str, value: str) -> None:
        self._connection.execute(
            """
            INSERT INTO memory_values (key, value, updated_at_unix_ms)
            VALUES (?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at_unix_ms = excluded.updated_at_unix_ms
            """,
            (key, value, _unix_ms()),
        )

    def _persist_metadata(self, key: str, value: object) -> None:
        self._connection.execute(
            """
            INSERT INTO memory_metadata (key, value)
            VALUES (?, ?)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            """,
            (key, json.dumps(value, separators=(",", ":"))),
        )

    def _metadata_bool(self, key: str) -> bool:
        row = self._connection.execute(
            "SELECT value FROM memory_metadata WHERE key = ?",
            (key,),
        ).fetchone()
        if row is None:
            return False
        return bool(json.loads(str(row["value"])))

    def _persist_event(self, event: DesktopStateEvent) -> None:
        self._connection.execute(
            """
            INSERT INTO desktop_state_events (
                occurred_at_unix_ms,
                source,
                kind,
                target_id,
                event_json
            )
            VALUES (?, ?, ?, ?, ?)
            """,
            (
                event.occurred_at_unix_ms,
                event.source,
                event.kind,
                event.target_id,
                event.to_json(),
            ),
        )

    def _persist_invalidation(self, invalidation: DesktopGraphInvalidation) -> None:
        self._connection.execute(
            """
            INSERT INTO desktop_graph_invalidations (
                occurred_at_unix_ms,
                invalidated_snapshot_id,
                reason,
                invalidation_json
            )
            VALUES (?, ?, ?, ?)
            """,
            (
                invalidation.event.occurred_at_unix_ms,
                invalidation.invalidated_snapshot_id,
                invalidation.reason,
                invalidation.to_json(),
            ),
        )

    def _persist_snapshot(self, snapshot: DesktopGraphSnapshot) -> None:
        self._connection.execute(
            """
            INSERT INTO desktop_graph_snapshots (
                id,
                captured_at_unix_ms,
                active_window_id,
                snapshot_json
            )
            VALUES (?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                captured_at_unix_ms = excluded.captured_at_unix_ms,
                active_window_id = excluded.active_window_id,
                snapshot_json = excluded.snapshot_json
            """,
            (
                snapshot.id,
                snapshot.captured_at_unix_ms,
                snapshot.active_window_id,
                snapshot.to_json(),
            ),
        )
        self._connection.execute(
            "DELETE FROM desktop_graph_nodes WHERE snapshot_id = ?",
            (snapshot.id,),
        )
        self._connection.execute(
            "DELETE FROM desktop_graph_edges WHERE snapshot_id = ?",
            (snapshot.id,),
        )
        self._connection.executemany(
            """
            INSERT INTO desktop_graph_nodes (
                snapshot_id,
                node_id,
                kind,
                label,
                role,
                x,
                y,
                width,
                height,
                attributes_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            [_node_row(snapshot.id, node) for node in snapshot.nodes],
        )
        self._connection.executemany(
            """
            INSERT INTO desktop_graph_edges (
                snapshot_id,
                edge_index,
                source,
                target,
                kind,
                attributes_json
            )
            VALUES (?, ?, ?, ?, ?, ?)
            """,
            [_edge_row(snapshot.id, index, edge) for index, edge in enumerate(snapshot.edges)],
        )


def _node_row(snapshot_id: str, node: GraphNode) -> tuple[object, ...]:
    bounds = node.bounds
    return (
        snapshot_id,
        node.id,
        node.kind,
        node.label,
        node.role,
        bounds.x if bounds else None,
        bounds.y if bounds else None,
        bounds.width if bounds else None,
        bounds.height if bounds else None,
        _json_dumps(node.attributes),
    )


def _edge_row(snapshot_id: str, index: int, edge: GraphEdge) -> tuple[object, ...]:
    return (
        snapshot_id,
        index,
        edge.source,
        edge.target,
        edge.kind,
        _json_dumps(edge.attributes),
    )


def _json_dumps(value: dict[str, object]) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def _unix_ms() -> int:
    return int(time.time() * 1000)
