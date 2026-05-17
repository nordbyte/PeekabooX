from __future__ import annotations

import json
import os
import subprocess
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable, Mapping


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
class PluginExecutionPolicy:
    timeout_seconds: float = 10.0
    max_output_bytes: int = 1_048_576
    environment: Mapping[str, str] = field(default_factory=dict)


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
    max_output_bytes: int = 1_048_576,
    environment: Mapping[str, str] | None = None,
) -> PluginToolExecutionResult:
    if plugin.manifest.entrypoint is None:
        raise ValueError(f"plugin {plugin.manifest.id!r} does not declare an entrypoint")
    plugin_tool = next((tool for tool in plugin.manifest.tools if tool.name == tool_name), None)
    if plugin_tool is None:
        raise ValueError(f"plugin {plugin.manifest.id!r} does not declare tool {tool_name!r}")
    if timeout_seconds <= 0:
        raise ValueError("timeout_seconds must be positive")
    if max_output_bytes < 0:
        raise ValueError("max_output_bytes must be non-negative")
    plugin_arguments = dict(arguments or {})
    validate_json_schema(plugin_tool.input_schema, plugin_arguments)
    policy = PluginExecutionPolicy(
        timeout_seconds=timeout_seconds,
        max_output_bytes=max_output_bytes,
        environment=dict(environment or {}),
    )
    request = {
        "schema_version": PLUGIN_SDK_VERSION,
        "plugin_id": plugin.manifest.id,
        "tool": tool_name,
        "arguments": plugin_arguments,
    }
    request_bytes = json.dumps(request).encode("utf-8")
    process = subprocess.Popen(
        plugin.manifest.entrypoint.command,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=plugin.root_dir,
        env=_plugin_environment(plugin, tool_name, policy.environment),
    )
    stdout_reader = _PipeReader(process.stdout, policy.max_output_bytes)
    stderr_reader = _PipeReader(process.stderr, policy.max_output_bytes)
    stdout_reader.start()
    stderr_reader.start()
    writer_error: list[BaseException] = []

    def write_request() -> None:
        try:
            assert process.stdin is not None
            process.stdin.write(request_bytes)
            process.stdin.flush()
            process.stdin.close()
        except BaseException as error:  # pragma: no cover - surfaced after process exit.
            writer_error.append(error)
            if process.stdin is not None:
                process.stdin.close()

    writer = threading.Thread(target=write_request, daemon=True)
    writer.start()
    timed_out = False
    try:
        deadline = time.monotonic() + policy.timeout_seconds
        while process.poll() is None:
            if time.monotonic() >= deadline:
                timed_out = True
                process.kill()
                break
            time.sleep(0.01)
        return_code = process.wait(timeout=2)
    finally:
        writer.join(timeout=2)
        stdout_reader.join(timeout=2)
        stderr_reader.join(timeout=2)

    stdout_bytes = stdout_reader.data
    stderr_bytes = stderr_reader.data
    stdout = stdout_bytes.decode("utf-8", errors="replace")
    stderr = stderr_bytes.decode("utf-8", errors="replace")

    if timed_out:
        return PluginToolExecutionResult(
            ok=False,
            plugin_id=plugin.manifest.id,
            tool=tool_name,
            exit_code=-1,
            stdout=stdout,
            stderr=stderr,
            result=None,
            error=f"plugin timed out after {policy.timeout_seconds:g} seconds",
        )
    if writer_error:
        raise ValueError(f"failed to write plugin request: {writer_error[0]}")
    if stdout_reader.error is not None:
        raise ValueError(f"failed to read plugin stdout: {stdout_reader.error}")
    if stderr_reader.error is not None:
        raise ValueError(f"failed to read plugin stderr: {stderr_reader.error}")

    stdout_too_large = stdout_reader.truncated
    stderr_too_large = stderr_reader.truncated
    payload = _parse_stdout_json(stdout)
    ok = return_code == 0 and not (
        isinstance(payload, dict) and payload.get("ok") is False
    )
    result = payload.get("result") if isinstance(payload, dict) else payload
    error = None
    if stdout_too_large or stderr_too_large:
        ok = False
        error = f"plugin output exceeded max_output_bytes={policy.max_output_bytes}"
    elif not ok:
        if isinstance(payload, dict) and payload.get("error") is not None:
            error = str(payload["error"])
        elif stderr:
            error = stderr.strip()
        else:
            error = f"plugin exited with status {return_code}"
    return PluginToolExecutionResult(
        ok=ok,
        plugin_id=plugin.manifest.id,
        tool=tool_name,
        exit_code=return_code,
        stdout=stdout,
        stderr=stderr,
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


def validate_json_schema(schema: dict[str, Any], value: Any) -> None:
    _validate_json_schema_at(schema, value, "$")


def _validate_json_schema_at(schema: Any, value: Any, path: str) -> None:
    if not isinstance(schema, dict):
        raise ValueError(f"{path}: schema must be an object")

    if "enum" in schema:
        enum_values = schema["enum"]
        if not isinstance(enum_values, list):
            raise ValueError(f"{path}: enum must be an array")
        if value not in enum_values:
            raise ValueError(f"{path}: value is not one of the allowed enum values")

    if "type" in schema and not _matches_schema_type(value, schema["type"]):
        raise ValueError(f"{path}: value does not match schema type {schema['type']!r}")

    if isinstance(value, dict):
        _validate_object_schema(schema, value, path)
    elif schema.get("required"):
        raise ValueError(f"{path}: required fields need an object value")

    if isinstance(value, list):
        if "minItems" in schema and len(value) < int(schema["minItems"]):
            raise ValueError(f"{path}: array has fewer than minItems {schema['minItems']}")
        if "maxItems" in schema and len(value) > int(schema["maxItems"]):
            raise ValueError(f"{path}: array has more than maxItems {schema['maxItems']}")
        if "items" in schema:
            for index, item in enumerate(value):
                _validate_json_schema_at(schema["items"], item, f"{path}[{index}]")

    if isinstance(value, str):
        if "minLength" in schema and len(value) < int(schema["minLength"]):
            raise ValueError(f"{path}: string is shorter than minLength {schema['minLength']}")
        if "maxLength" in schema and len(value) > int(schema["maxLength"]):
            raise ValueError(f"{path}: string is longer than maxLength {schema['maxLength']}")

    if isinstance(value, int | float) and not isinstance(value, bool):
        if "minimum" in schema and value < float(schema["minimum"]):
            raise ValueError(f"{path}: value is smaller than minimum {schema['minimum']}")
        if "maximum" in schema and value > float(schema["maximum"]):
            raise ValueError(f"{path}: value is greater than maximum {schema['maximum']}")


def _matches_schema_type(value: Any, expected: Any) -> bool:
    if isinstance(expected, list):
        return any(_matches_schema_type(value, item) for item in expected)
    if expected == "object":
        return isinstance(value, dict)
    if expected == "array":
        return isinstance(value, list)
    if expected == "string":
        return isinstance(value, str)
    if expected == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if expected == "number":
        return isinstance(value, int | float) and not isinstance(value, bool)
    if expected == "boolean":
        return isinstance(value, bool)
    if expected == "null":
        return value is None
    return False


def _validate_object_schema(schema: dict[str, Any], value: dict[str, Any], path: str) -> None:
    required = schema.get("required", [])
    if not isinstance(required, list) or not all(isinstance(item, str) for item in required):
        raise ValueError(f"{path}: required must be a string array")
    for field in required:
        if field not in value:
            raise ValueError(f"{path}.{field}: required field is missing")

    properties = schema.get("properties", {})
    if properties is None:
        properties = {}
    if not isinstance(properties, dict):
        raise ValueError(f"{path}: properties must be an object")
    for field, field_schema in properties.items():
        if field in value:
            _validate_json_schema_at(field_schema, value[field], f"{path}.{field}")

    if schema.get("additionalProperties") is False:
        for field in value:
            if field not in properties:
                raise ValueError(f"{path}.{field}: additional property is not allowed")


SAFE_PLUGIN_ENV = (
    "PATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "PYTHONPATH",
    "PYTHONHOME",
    "VIRTUAL_ENV",
    "XDG_RUNTIME_DIR",
)


def _plugin_environment(
    plugin: PluginDescriptor,
    tool_name: str,
    extra: Mapping[str, str],
) -> dict[str, str]:
    environment = {
        key: os.environ[key]
        for key in SAFE_PLUGIN_ENV
        if key in os.environ
    }
    environment.update({str(key): str(value) for key, value in extra.items()})
    environment["PEEKABOOX_PLUGIN_ID"] = plugin.manifest.id
    environment["PEEKABOOX_PLUGIN_TOOL"] = tool_name
    environment["PEEKABOOX_PLUGIN_ROOT"] = str(plugin.root_dir)
    return environment


class _PipeReader:
    def __init__(self, stream: Any, max_bytes: int) -> None:
        self._stream = stream
        self._max_bytes = max_bytes
        self._chunks: list[bytes] = []
        self._total_bytes = 0
        self._stored_bytes = 0
        self.error: BaseException | None = None
        self._thread = threading.Thread(target=self._run, daemon=True)

    def start(self) -> None:
        self._thread.start()

    def join(self, timeout: float | None = None) -> None:
        self._thread.join(timeout)

    @property
    def data(self) -> bytes:
        return b"".join(self._chunks)

    @property
    def truncated(self) -> bool:
        return self._total_bytes > self._max_bytes

    def _run(self) -> None:
        if self._stream is None:
            return
        try:
            while True:
                chunk = self._stream.read(8192)
                if not chunk:
                    break
                self._total_bytes += len(chunk)
                if self._stored_bytes < self._max_bytes:
                    remaining = self._max_bytes - self._stored_bytes
                    self._chunks.append(chunk[:remaining])
                    self._stored_bytes += min(len(chunk), remaining)
        except BaseException as error:  # pragma: no cover - surfaced by execute_plugin_tool.
            self.error = error
        finally:
            close = getattr(self._stream, "close", None)
            if close is not None:
                close()


def _parse_stdout_json(stdout: str) -> Any:
    text = stdout.strip()
    if not text:
        return None
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return text
