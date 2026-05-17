from __future__ import annotations

import json
import time
from dataclasses import dataclass, field

from peekaboox.memory.graph import DesktopGraphSnapshot


@dataclass(frozen=True, slots=True)
class DesktopStateEvent:
    kind: str
    source: str
    occurred_at_unix_ms: int
    target_id: str | None = None
    payload: dict[str, object] = field(default_factory=dict)

    @classmethod
    def create(
        cls,
        kind: str,
        source: str = "runtime",
        occurred_at_unix_ms: int | None = None,
        target_id: str | None = None,
        payload: dict[str, object] | None = None,
    ) -> DesktopStateEvent:
        if not kind:
            raise ValueError("desktop event kind must not be empty")
        if not source:
            raise ValueError("desktop event source must not be empty")
        return cls(
            kind=kind,
            source=source,
            occurred_at_unix_ms=occurred_at_unix_ms or _unix_ms(),
            target_id=target_id,
            payload=payload or {},
        )

    def to_dict(self) -> dict[str, object]:
        return {
            "kind": self.kind,
            "source": self.source,
            "occurred_at_unix_ms": self.occurred_at_unix_ms,
            "target_id": self.target_id,
            "payload": self.payload,
        }

    @classmethod
    def from_dict(cls, value: dict[str, object]) -> DesktopStateEvent:
        payload = value.get("payload", {})
        if not isinstance(payload, dict):
            raise ValueError("event payload must be an object")
        return cls(
            kind=str(value["kind"]),
            source=str(value["source"]),
            occurred_at_unix_ms=int(value["occurred_at_unix_ms"]),
            target_id=str(value["target_id"]) if value.get("target_id") is not None else None,
            payload=dict(payload),
        )

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), sort_keys=True, separators=(",", ":"))

    @classmethod
    def from_json(cls, value: str) -> DesktopStateEvent:
        decoded = json.loads(value)
        if not isinstance(decoded, dict):
            raise ValueError("desktop event JSON must decode to an object")
        return cls.from_dict(decoded)


@dataclass(frozen=True, slots=True)
class DesktopGraphInvalidation:
    event: DesktopStateEvent
    invalidated_snapshot_id: str | None
    affected_node_ids: tuple[str, ...]
    reason: str
    requires_refresh: bool = True

    def to_dict(self) -> dict[str, object]:
        return {
            "event": self.event.to_dict(),
            "invalidated_snapshot_id": self.invalidated_snapshot_id,
            "affected_node_ids": list(self.affected_node_ids),
            "reason": self.reason,
            "requires_refresh": self.requires_refresh,
        }

    @classmethod
    def from_dict(cls, value: dict[str, object]) -> DesktopGraphInvalidation:
        event = value.get("event")
        affected_node_ids = value.get("affected_node_ids", [])
        if not isinstance(event, dict):
            raise ValueError("invalidation event must be an object")
        if not isinstance(affected_node_ids, list):
            raise ValueError("affected_node_ids must be a list")
        return cls(
            event=DesktopStateEvent.from_dict(event),
            invalidated_snapshot_id=(
                str(value["invalidated_snapshot_id"])
                if value.get("invalidated_snapshot_id") is not None
                else None
            ),
            affected_node_ids=tuple(str(node_id) for node_id in affected_node_ids),
            reason=str(value["reason"]),
            requires_refresh=bool(value.get("requires_refresh", True)),
        )

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), sort_keys=True, separators=(",", ":"))

    @classmethod
    def from_json(cls, value: str) -> DesktopGraphInvalidation:
        decoded = json.loads(value)
        if not isinstance(decoded, dict):
            raise ValueError("desktop graph invalidation JSON must decode to an object")
        return cls.from_dict(decoded)


@dataclass(frozen=True, slots=True)
class DesktopGraphUpdate:
    event: DesktopStateEvent
    stale: bool
    invalidation: DesktopGraphInvalidation | None = None
    snapshot: DesktopGraphSnapshot | None = None


@dataclass(frozen=True, slots=True)
class DesktopGraphStatus:
    stale: bool
    latest_snapshot_id: str | None
    event_count: int
    invalidation_count: int
    last_event: DesktopStateEvent | None = None
    last_invalidation: DesktopGraphInvalidation | None = None


def _unix_ms() -> int:
    return int(time.time() * 1000)
