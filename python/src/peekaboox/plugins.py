from __future__ import annotations

import json
import os
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable


PLUGIN_SDK_VERSION = "peekaboox.plugin.v1"
PLUGIN_MANIFEST_FILE = "peekaboox.plugin.json"
PLUGIN_PATH_ENV = "PEEKABOOX_PLUGIN_PATH"


@dataclass(frozen=True, slots=True)
class PluginEntrypoint:
    kind: str
    command: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class PluginTool:
    name: str
    description: str
    capabilities: tuple[str, ...] = ()
    input_schema: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class PluginManifest:
    schema_version: str
    id: str
    name: str
    version: str
    description: str | None = None
    capabilities: tuple[str, ...] = ()
    entrypoint: PluginEntrypoint | None = None
    tools: tuple[PluginTool, ...] = ()
    metadata: dict[str, str] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class PluginDescriptor:
    manifest: PluginManifest
    root_dir: Path
    manifest_path: Path


@dataclass(frozen=True, slots=True)
class PluginDiscoveryError:
    path: Path
    message: str


@dataclass(frozen=True, slots=True)
class PluginDiscoveryResult:
    sdk_version: str
    plugins: tuple[PluginDescriptor, ...]
    errors: tuple[PluginDiscoveryError, ...] = ()


@dataclass(frozen=True, slots=True)
class PluginToolExecutionResult:
    ok: bool
    plugin_id: str
    tool: str
    exit_code: int
    stdout: str
    stderr: str
    result: Any | None = None
    error: str | None = None


def default_plugin_search_paths() -> tuple[Path, ...]:
    return (*plugin_paths_from_env(), Path("plugins"))


def plugin_paths_from_env(variable: str = PLUGIN_PATH_ENV) -> tuple[Path, ...]:
    value = os.environ.get(variable)
    if not value:
        return ()
    return tuple(Path(path) for path in value.split(os.pathsep) if path)


def discover_plugins(paths: Iterable[str | os.PathLike[str]] | None = None) -> PluginDiscoveryResult:
    search_paths = tuple(Path(path) for path in paths) if paths is not None else default_plugin_search_paths()
    plugins: list[PluginDescriptor] = []
    errors: list[PluginDiscoveryError] = []

    for path in search_paths:
        _discover_path(path, plugins, errors)

    return PluginDiscoveryResult(
        sdk_version=PLUGIN_SDK_VERSION,
        plugins=tuple(sorted(plugins, key=lambda plugin: plugin.manifest.id)),
        errors=tuple(sorted(errors, key=lambda error: str(error.path))),
    )


def load_plugin(path: str | os.PathLike[str]) -> PluginDescriptor:
    manifest_path = _manifest_path(Path(path))
    try:
        payload = json.loads(manifest_path.read_text(encoding="utf-8"))
    except OSError as error:
        raise ValueError(f"failed to read {manifest_path}: {error}") from error
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid plugin manifest {manifest_path}: {error.msg}") from error

    if not isinstance(payload, dict):
        raise ValueError(f"invalid plugin manifest {manifest_path}: root must be an object")
    manifest = _manifest_from_dict(payload)
    validate_manifest(manifest)
    return PluginDescriptor(
        manifest=manifest,
        root_dir=manifest_path.parent,
        manifest_path=manifest_path,
    )


def validate_manifest(manifest: PluginManifest) -> None:
    if manifest.schema_version != PLUGIN_SDK_VERSION:
        raise ValueError(
            f"unsupported plugin schema_version {manifest.schema_version!r}; "
            f"expected {PLUGIN_SDK_VERSION!r}"
        )
    _validate_identifier("plugin id", manifest.id)
    _validate_non_empty("plugin name", manifest.name)
    _validate_non_empty("plugin version", manifest.version)
    for capability in manifest.capabilities:
        _validate_identifier("plugin capability", capability)
    if manifest.entrypoint is not None:
        if manifest.entrypoint.kind != "process":
            raise ValueError("plugin entrypoint.kind must be 'process'")
        if not manifest.entrypoint.command or any(not part for part in manifest.entrypoint.command):
            raise ValueError("plugin entrypoint.command must contain non-empty command parts")
    for tool in manifest.tools:
        _validate_identifier("plugin tool name", tool.name)
        _validate_non_empty("plugin tool description", tool.description)
        if not isinstance(tool.input_schema, dict):
            raise ValueError(f"plugin tool {tool.name!r} input_schema must be an object")
        for capability in tool.capabilities:
            _validate_identifier("plugin capability", capability)


def execute_plugin_tool(
    plugin: PluginDescriptor,
    tool_name: str,
    arguments: dict[str, Any] | None = None,
    *,
    timeout_seconds: float = 10.0,
) -> PluginToolExecutionResult:
    if plugin.manifest.entrypoint is None:
        raise ValueError(f"plugin {plugin.manifest.id!r} does not declare an entrypoint")
    if not any(tool.name == tool_name for tool in plugin.manifest.tools):
        raise ValueError(f"plugin {plugin.manifest.id!r} does not declare tool {tool_name!r}")
    request = {
        "schema_version": PLUGIN_SDK_VERSION,
        "plugin_id": plugin.manifest.id,
        "tool": tool_name,
        "arguments": arguments or {},
    }
    completed = subprocess.run(
        plugin.manifest.entrypoint.command,
        input=json.dumps(request),
        text=True,
        capture_output=True,
        cwd=plugin.root_dir,
        timeout=timeout_seconds,
        check=False,
    )
    payload = _parse_stdout_json(completed.stdout)
    ok = completed.returncode == 0 and not (
        isinstance(payload, dict) and payload.get("ok") is False
    )
    result = payload.get("result") if isinstance(payload, dict) else payload
    error = None
    if not ok:
        if isinstance(payload, dict) and payload.get("error") is not None:
            error = str(payload["error"])
        elif completed.stderr:
            error = completed.stderr.strip()
        else:
            error = f"plugin exited with status {completed.returncode}"
    return PluginToolExecutionResult(
        ok=ok,
        plugin_id=plugin.manifest.id,
        tool=tool_name,
        exit_code=completed.returncode,
        stdout=completed.stdout,
        stderr=completed.stderr,
        result=result,
        error=error,
    )


def _discover_path(
    path: Path,
    plugins: list[PluginDescriptor],
    errors: list[PluginDiscoveryError],
) -> None:
    if not path.exists():
        return
    if path.is_file() or (path / PLUGIN_MANIFEST_FILE).is_file():
        _push_plugin(path, plugins, errors)
        return
    try:
        entries = sorted(path.iterdir())
    except OSError as error:
        errors.append(PluginDiscoveryError(path=path, message=str(error)))
        return
    for entry in entries:
        if entry.is_dir() and (entry / PLUGIN_MANIFEST_FILE).is_file():
            _push_plugin(entry, plugins, errors)
        elif entry.is_file() and entry.name == PLUGIN_MANIFEST_FILE:
            _push_plugin(entry, plugins, errors)


def _push_plugin(
    path: Path,
    plugins: list[PluginDescriptor],
    errors: list[PluginDiscoveryError],
) -> None:
    try:
        plugins.append(load_plugin(path))
    except ValueError as error:
        errors.append(PluginDiscoveryError(path=path, message=str(error)))


def _manifest_path(path: Path) -> Path:
    return path / PLUGIN_MANIFEST_FILE if path.is_dir() else path


def _manifest_from_dict(payload: dict[str, Any]) -> PluginManifest:
    entrypoint_payload = payload.get("entrypoint")
    entrypoint = None
    if entrypoint_payload is not None:
        if not isinstance(entrypoint_payload, dict):
            raise ValueError("plugin entrypoint must be an object")
        command = entrypoint_payload.get("command", [])
        if not isinstance(command, list) or not all(isinstance(part, str) for part in command):
            raise ValueError("plugin entrypoint.command must be a string array")
        entrypoint = PluginEntrypoint(
            kind=str(entrypoint_payload.get("kind", "")),
            command=tuple(command),
        )

    return PluginManifest(
        schema_version=_required_str(payload, "schema_version"),
        id=_required_str(payload, "id"),
        name=_required_str(payload, "name"),
        version=_required_str(payload, "version"),
        description=_optional_str(payload, "description"),
        capabilities=_string_tuple(payload.get("capabilities", []), "capabilities"),
        entrypoint=entrypoint,
        tools=tuple(_tool_from_dict(tool) for tool in _object_list(payload.get("tools", []), "tools")),
        metadata={
            str(key): str(value)
            for key, value in (payload.get("metadata") or {}).items()
        }
        if isinstance(payload.get("metadata") or {}, dict)
        else {},
    )


def _tool_from_dict(payload: dict[str, Any]) -> PluginTool:
    return PluginTool(
        name=_required_str(payload, "name"),
        description=_required_str(payload, "description"),
        capabilities=_string_tuple(payload.get("capabilities", []), "tool.capabilities"),
        input_schema=payload.get("input_schema") or {
            "type": "object",
            "properties": {},
            "additionalProperties": False,
        },
    )


def _required_str(payload: dict[str, Any], name: str) -> str:
    value = payload.get(name)
    if not isinstance(value, str) or not value:
        raise ValueError(f"plugin {name} must be a non-empty string")
    return value


def _optional_str(payload: dict[str, Any], name: str) -> str | None:
    value = payload.get(name)
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(f"plugin {name} must be a string")
    return value


def _string_tuple(value: Any, name: str) -> tuple[str, ...]:
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise ValueError(f"plugin {name} must be a string array")
    return tuple(value)


def _object_list(value: Any, name: str) -> list[dict[str, Any]]:
    if not isinstance(value, list) or not all(isinstance(item, dict) for item in value):
        raise ValueError(f"plugin {name} must be an object array")
    return value


def _validate_non_empty(label: str, value: str) -> None:
    if not value.strip():
        raise ValueError(f"{label} must not be empty")


def _validate_identifier(label: str, value: str) -> None:
    _validate_non_empty(label, value)
    allowed = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-")
    if len(value) > 128:
        raise ValueError(f"{label} must be 128 characters or shorter")
    if any(character not in allowed for character in value):
        raise ValueError(
            f"{label} {value!r} must use only ASCII letters, digits, dots, underscores, or dashes"
        )


def _parse_stdout_json(stdout: str) -> Any:
    text = stdout.strip()
    if not text:
        return None
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return text
