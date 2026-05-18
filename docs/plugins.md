# Plugin SDK

PeekabooX plugins are directory-based packages with a checked manifest named
`peekaboox.plugin.json`. The SDK discovers plugins, validates their
capabilities and tool metadata, and exposes them through CLI, daemon JSON IPC,
gRPC, Python runtime, and MCP. Declared process tools can be executed through
CLI `plugin-call`, daemon JSON IPC, gRPC, Python runtime, and MCP.

## Manifest

```json
{
  "schema_version": "peekaboox.plugin.v1",
  "id": "org.example.plugin",
  "name": "Example Plugin",
  "version": "1.0.0",
  "description": "Optional human-readable summary.",
  "capabilities": ["observe"],
  "entrypoint": {
    "kind": "process",
    "command": ["python3", "plugin.py"]
  },
  "tools": [
    {
      "name": "example.inspect",
      "description": "Inspect example state.",
      "capabilities": ["observe"],
      "input_schema": {
        "type": "object",
        "properties": {},
        "additionalProperties": false
      }
    }
  ],
  "metadata": {
    "homepage": "https://github.com/nordbyte/PeekabooX/tree/main/examples/plugins/system-info"
  }
}
```

`id`, tool names, and capability names must use ASCII letters, digits, `.`, `_`,
or `-`. `schema_version` must be `peekaboox.plugin.v1`.

## Discovery

PeekabooX discovers plugins from explicit paths, `PEEKABOOX_PLUGIN_PATH`, and
the local `plugins` directory. A path may point to a manifest file, one plugin
directory, or a directory containing multiple plugin directories.

```bash
peekaboox plugins --path examples/plugins
peekaboox plugins --path examples/plugins --json
peekaboox plugin-call org.peekaboox.examples.system-info system_info.uname --path examples/plugins --json
examples/cli/plugins_system_info.sh
peekabooxd run --plugin-path examples/plugins
peekaboox --daemon plugins
peekaboox --daemon plugin-call org.peekaboox.examples.system-info system_info.uname
```

The Python runtime exposes the same contract:

```python
from peekaboox.agent import AgentRuntime

runtime = AgentRuntime(plugin_paths=("examples/plugins",))
plugins = runtime.list_plugins()
result = runtime.call_plugin_tool(
    "org.peekaboox.examples.system-info",
    "system_info.uname",
)
```

MCP exposes `list_plugins` with an optional `paths` array. It also exposes
`call_plugin_tool` with `plugin_id`, `tool`, optional `arguments`, optional
`paths`, optional `timeout_seconds`, optional `max_output_bytes`, optional
`require_trusted`, and optional `trust_policy`. Python and MCP execution is
gated by the `plugin_execute` runtime capability.
Daemon-routed plugin execution is gated separately from input automation; start
`peekabooxd` with `--profile operator`, `--allow-plugins`, or
`PEEKABOOX_ALLOW_PLUGINS=1`.
Use `examples/python/plugins_runtime.py` and `examples/mcp/jsonrpc_plugins.sh`
to validate the same discovery and execution path through the Python runtime
and MCP JSON-RPC.

## Trust Policy

PeekabooX can require plugin manifests to match a pinned SHA-256 fingerprint
before execution. A trust policy is a JSON file:

```json
{
  "schema_version": 1,
  "plugins": {
    "org.peekaboox.examples.system-info": {
      "manifest_sha256": "<sha256>"
    }
  }
}
```

Set `PEEKABOOX_REQUIRE_TRUSTED_PLUGINS=1` for Python runtime execution, pass
`require_trusted=True` to `AgentRuntime.call_plugin_tool(...)`, or use:

```bash
peekaboox plugin-call org.peekaboox.examples.system-info system_info.uname \
  --path examples/plugins \
  --require-trusted \
  --trust-policy trusted_plugins.json \
  --json
```

If `--trust-policy` is omitted, the default path is
`$XDG_CONFIG_HOME/peekaboox/trusted_plugins.json` or
`~/.config/peekaboox/trusted_plugins.json`.

## Execution Safety

Before a process tool starts, PeekabooX validates `arguments` against the
tool's `input_schema` subset: `type`, `required`, `properties`,
`additionalProperties: false`, `enum`, scalar min/max bounds, string length,
and array item bounds. Process tools run with a restricted inherited
environment plus `PEEKABOOX_PLUGIN_ID`, `PEEKABOOX_PLUGIN_TOOL`, and
`PEEKABOOX_PLUGIN_ROOT`. Timeouts return structured execution errors, and
stdout/stderr are truncated to `max_output_bytes` before being surfaced.

## Example

`examples/plugins/system-info` is a minimal read-only process plugin. Its
`plugin.py` demonstrates the process contract: JSON request on stdin, JSON
response on stdout, and non-zero exit code on tool errors. The request includes
`schema_version`, `plugin_id`, `tool`, and `arguments`.
The dedicated CLI, Python, and MCP examples exercise this plugin end to end
without mutating the desktop.
