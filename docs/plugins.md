# Plugin SDK

PeekabooX plugins are directory-based packages with a checked manifest named
`peekaboox.plugin.json`. The SDK discovers plugins, validates their
capabilities and tool metadata, and exposes them through CLI, daemon JSON IPC,
Python runtime, and MCP. Python/MCP also include a process adapter for executing
declared tools.

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
    "homepage": "https://git.marketdeck.io/marketdeck/PeekabooX/src/branch/main/examples/plugins/system-info"
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
peekabooxd run --plugin-path examples/plugins
peekaboox --daemon plugins
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
`paths`, and optional `timeout_seconds`. Execution is gated by the
`plugin_execute` runtime capability.

## Example

`examples/plugins/system-info` is a minimal read-only process plugin. Its
`plugin.py` demonstrates the process contract: JSON request on stdin, JSON
response on stdout, and non-zero exit code on tool errors. The request includes
`schema_version`, `plugin_id`, `tool`, and `arguments`.
