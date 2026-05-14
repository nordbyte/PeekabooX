from __future__ import annotations

import json
import os
import time
from collections.abc import Callable, Iterable
from dataclasses import dataclass, field
from pathlib import Path
from threading import Lock


class CapabilityDeniedError(PermissionError):
    def __init__(self, capability: str, operation: str) -> None:
        super().__init__(f"capability denied: {capability} for {operation}")
        self.capability = capability
        self.operation = operation


class ConfirmationRequiredError(PermissionError):
    def __init__(self, action: str, operation: str) -> None:
        super().__init__(f"confirmation required: {action} for {operation}")
        self.action = action
        self.operation = operation


class ConfirmationDeniedError(PermissionError):
    def __init__(self, action: str, operation: str) -> None:
        super().__init__(f"confirmation denied: {action} for {operation}")
        self.action = action
        self.operation = operation


class Capability:
    OBSERVE = "observe"
    CLICK = "click"
    TYPE_TEXT = "type_text"
    WORKFLOW_EXECUTE = "workflow_execute"
    WORKFLOW_RECORD = "workflow_record"
    WORKFLOW_GENERATE = "workflow_generate"
    VISION = "vision"
    MEMORY_READ = "memory_read"
    MEMORY_WRITE = "memory_write"
    PLUGIN_READ = "plugin_read"
    PLUGIN_EXECUTE = "plugin_execute"


ALL_CAPABILITIES = frozenset(
    {
        Capability.OBSERVE,
        Capability.CLICK,
        Capability.TYPE_TEXT,
        Capability.WORKFLOW_EXECUTE,
        Capability.WORKFLOW_RECORD,
        Capability.WORKFLOW_GENERATE,
        Capability.VISION,
        Capability.MEMORY_READ,
        Capability.MEMORY_WRITE,
        Capability.PLUGIN_READ,
        Capability.PLUGIN_EXECUTE,
    }
)


class CapabilityProfile:
    OBSERVE = "observe"
    PLAN = "plan"
    ASSIST = "assist"
    OPERATOR = "operator"


@dataclass(frozen=True, slots=True)
class CapabilityProfileSpec:
    name: str
    capabilities: frozenset[str]
    description: str


CAPABILITY_PROFILES: dict[str, CapabilityProfileSpec] = {
    CapabilityProfile.OBSERVE: CapabilityProfileSpec(
        name=CapabilityProfile.OBSERVE,
        capabilities=frozenset(
            {
                Capability.OBSERVE,
                Capability.VISION,
                Capability.MEMORY_READ,
                Capability.PLUGIN_READ,
            }
        ),
        description="read-only desktop observation, vision, and memory reads",
    ),
    CapabilityProfile.PLAN: CapabilityProfileSpec(
        name=CapabilityProfile.PLAN,
        capabilities=frozenset(
            {
                Capability.OBSERVE,
                Capability.VISION,
                Capability.MEMORY_READ,
                Capability.MEMORY_WRITE,
                Capability.WORKFLOW_GENERATE,
                Capability.PLUGIN_READ,
            }
        ),
        description="observation, vision, memory updates, and workflow generation",
    ),
    CapabilityProfile.ASSIST: CapabilityProfileSpec(
        name=CapabilityProfile.ASSIST,
        capabilities=frozenset(
            {
                Capability.OBSERVE,
                Capability.CLICK,
                Capability.WORKFLOW_EXECUTE,
                Capability.WORKFLOW_GENERATE,
                Capability.VISION,
                Capability.MEMORY_READ,
                Capability.MEMORY_WRITE,
                Capability.PLUGIN_READ,
            }
        ),
        description="interactive assistance without text typing or recording",
    ),
    CapabilityProfile.OPERATOR: CapabilityProfileSpec(
        name=CapabilityProfile.OPERATOR,
        capabilities=ALL_CAPABILITIES,
        description="full runtime capability set",
    ),
}

KNOWN_CAPABILITY_PROFILES = tuple(CAPABILITY_PROFILES)


def capability_profile(profile: str) -> CapabilityProfileSpec:
    normalized = profile.strip().lower().replace("_", "-")
    aliases = {
        "all": CapabilityProfile.OPERATOR,
        "default": CapabilityProfile.OPERATOR,
        "full": CapabilityProfile.OPERATOR,
        "planning": CapabilityProfile.PLAN,
        "read-only": CapabilityProfile.OBSERVE,
        "readonly": CapabilityProfile.OBSERVE,
        "trusted": CapabilityProfile.OPERATOR,
    }
    normalized = aliases.get(normalized, normalized)
    if normalized not in CAPABILITY_PROFILES:
        known = ", ".join(KNOWN_CAPABILITY_PROFILES)
        raise ValueError(f"unknown capability profile {profile!r}; expected one of: {known}")
    return CAPABILITY_PROFILES[normalized]


class DangerousAction:
    CLICK = "click"
    TYPE_TEXT = "type_text"
    WORKFLOW_EXECUTE = "workflow_execute"


ALL_DANGEROUS_ACTIONS = frozenset(
    {
        DangerousAction.CLICK,
        DangerousAction.TYPE_TEXT,
        DangerousAction.WORKFLOW_EXECUTE,
    }
)


@dataclass(frozen=True, slots=True)
class CapabilityAuditEvent:
    capability: str
    operation: str
    allowed: bool
    occurred_at_unix_ms: int
    metadata: dict[str, object] = field(default_factory=dict)


@dataclass(slots=True)
class JsonlAuditLogger:
    path: str | os.PathLike[str]
    source: str = "runtime"
    _lock: Lock = field(default_factory=Lock, init=False, repr=False)

    def __post_init__(self) -> None:
        self.path = Path(self.path)
        if self.path.parent:
            self.path.parent.mkdir(parents=True, exist_ok=True)

    def write(
        self,
        event: str,
        status: str,
        details: dict[str, object],
        error: str | None = None,
    ) -> None:
        record = {
            "ts_unix_ms": _unix_ms(),
            "source": self.source,
            "event": event,
            "status": status,
            "error": error,
            "pid": os.getpid(),
            "details": details,
        }
        with self._lock:
            with self.path.open("a", encoding="utf-8") as file:
                json.dump(record, file, ensure_ascii=False, sort_keys=True)
                file.write("\n")


@dataclass(slots=True)
class CapabilityPolicy:
    allowed_capabilities: Iterable[str] = field(default_factory=lambda: ALL_CAPABILITIES)
    audit_events: list[CapabilityAuditEvent] = field(default_factory=list)
    audit_logger: JsonlAuditLogger | None = None

    def __post_init__(self) -> None:
        self.allowed_capabilities = frozenset(self.allowed_capabilities)

    @classmethod
    def allow_all(
        cls,
        audit_logger: JsonlAuditLogger | None = None,
    ) -> "CapabilityPolicy":
        return cls(allowed_capabilities=ALL_CAPABILITIES, audit_logger=audit_logger)

    @classmethod
    def allow_only(
        cls,
        capabilities: Iterable[str],
        audit_logger: JsonlAuditLogger | None = None,
    ) -> "CapabilityPolicy":
        return cls(allowed_capabilities=capabilities, audit_logger=audit_logger)

    @classmethod
    def deny(
        cls,
        capabilities: Iterable[str],
        audit_logger: JsonlAuditLogger | None = None,
    ) -> "CapabilityPolicy":
        denied = frozenset(capabilities)
        return cls(
            allowed_capabilities=ALL_CAPABILITIES - denied,
            audit_logger=audit_logger,
        )

    @classmethod
    def from_profile(
        cls,
        profile: str,
        audit_logger: JsonlAuditLogger | None = None,
    ) -> "CapabilityPolicy":
        return cls(
            allowed_capabilities=capability_profile(profile).capabilities,
            audit_logger=audit_logger,
        )

    @classmethod
    def from_env(
        cls,
        variable: str = "PEEKABOOX_CAPABILITY_PROFILE",
        audit_logger: JsonlAuditLogger | None = None,
    ) -> "CapabilityPolicy":
        profile = os.environ.get(variable)
        if profile is None:
            return cls.allow_all(audit_logger=audit_logger)
        return cls.from_profile(profile, audit_logger=audit_logger)

    def allows(self, capability: str) -> bool:
        return capability in self.allowed_capabilities

    def require(
        self,
        capability: str,
        operation: str,
        metadata: dict[str, object] | None = None,
    ) -> None:
        allowed = self.allows(capability)
        event = CapabilityAuditEvent(
            capability=capability,
            operation=operation,
            allowed=allowed,
            occurred_at_unix_ms=_unix_ms(),
            metadata=metadata or {},
        )
        self.audit_events.append(event)
        if self.audit_logger is not None:
            self.audit_logger.write(
                "capability",
                "ok" if allowed else "denied",
                {
                    "capability": event.capability,
                    "operation": event.operation,
                    "allowed": event.allowed,
                    "metadata": event.metadata,
                },
            )
        if not allowed:
            raise CapabilityDeniedError(capability, operation)


@dataclass(frozen=True, slots=True)
class ConfirmationRequest:
    action: str
    operation: str
    metadata: dict[str, object] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class ConfirmationAuditEvent:
    action: str
    operation: str
    required: bool
    confirmed: bool
    occurred_at_unix_ms: int
    metadata: dict[str, object] = field(default_factory=dict)


Confirmer = Callable[[ConfirmationRequest], bool]


@dataclass(slots=True)
class ConfirmationPolicy:
    required_actions: Iterable[str] = field(default_factory=tuple)
    confirmer: Confirmer | None = None
    audit_events: list[ConfirmationAuditEvent] = field(default_factory=list)
    audit_logger: JsonlAuditLogger | None = None

    def __post_init__(self) -> None:
        self.required_actions = frozenset(self.required_actions)

    @classmethod
    def disabled(
        cls,
        audit_logger: JsonlAuditLogger | None = None,
    ) -> "ConfirmationPolicy":
        return cls(audit_logger=audit_logger)

    @classmethod
    def require_for(
        cls,
        actions: Iterable[str],
        confirmer: Confirmer | None = None,
        audit_logger: JsonlAuditLogger | None = None,
    ) -> "ConfirmationPolicy":
        return cls(
            required_actions=actions,
            confirmer=confirmer,
            audit_logger=audit_logger,
        )

    @classmethod
    def require_all(
        cls,
        confirmer: Confirmer | None = None,
        audit_logger: JsonlAuditLogger | None = None,
    ) -> "ConfirmationPolicy":
        return cls(
            required_actions=ALL_DANGEROUS_ACTIONS,
            confirmer=confirmer,
            audit_logger=audit_logger,
        )

    def requires(self, action: str) -> bool:
        return action in self.required_actions

    def confirm(
        self,
        action: str,
        operation: str,
        metadata: dict[str, object] | None = None,
    ) -> None:
        request = ConfirmationRequest(
            action=action,
            operation=operation,
            metadata=metadata or {},
        )
        if not self.requires(action):
            return

        if self.confirmer is None:
            self._audit(request, required=True, confirmed=False)
            raise ConfirmationRequiredError(action, operation)

        confirmed = bool(self.confirmer(request))
        self._audit(request, required=True, confirmed=confirmed)
        if not confirmed:
            raise ConfirmationDeniedError(action, operation)

    def _audit(
        self,
        request: ConfirmationRequest,
        *,
        required: bool,
        confirmed: bool,
    ) -> None:
        self.audit_events.append(
            event := ConfirmationAuditEvent(
                action=request.action,
                operation=request.operation,
                required=required,
                confirmed=confirmed,
                occurred_at_unix_ms=_unix_ms(),
                metadata=request.metadata,
            )
        )
        if self.audit_logger is not None:
            self.audit_logger.write(
                "confirmation",
                "confirmed" if confirmed else "blocked",
                {
                    "action": event.action,
                    "operation": event.operation,
                    "required": event.required,
                    "confirmed": event.confirmed,
                    "metadata": event.metadata,
                },
            )


def _unix_ms() -> int:
    return int(time.time() * 1000)
