# PeekabooX Python Package

Python runtime, gRPC client, MCP server, workflow helpers, memory store, and
plugin SDK bindings for the PeekabooX Linux desktop automation platform.

Install from this repository with:

```bash
python3 -m pip install ./python
```

Build a wheel with:

```bash
python3 -m pip wheel --no-deps -w target/python-wheel ./python
```

Useful console entry points after installation:

```bash
peekaboox-agent --version
peekaboox-agent plugins --path examples/plugins
peekaboox-mcp --list-tools
```
