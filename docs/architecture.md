# Architecture

PeekabooX is split into a Linux-native Rust core and a Python agent layer.

The Rust side owns low-latency desktop integration:

- capture
- input injection
- window management
- accessibility
- vision primitives, including a Tesseract-backed OCR provider abstraction
- IPC contracts

The Python side owns agent orchestration:

- deterministic planning, graph-assisted workflow draft generation, optional
  provider-backed workflow refinement, semantic workflow recording, and
  self-healing workflow execution with retries, verification, and structured
  recovery metadata, including reusable JSON/YAML workflow files
- MCP integration through a transport-neutral tool registry and dispatcher bound
  to `AgentRuntime`, with stdio JSON-RPC transport for MCP clients
- runtime security policy for Python and MCP callers, including granular
  capability checks, reusable allowlist profiles, optional dangerous-action
  confirmations, and in-memory plus JSONL audit events layered above
  daemon-side input permission gates
- daemon-side emergency stop handling, including a best-effort Linux input
  hotkey listener for `CTRL + ALT + ESC` and backend modifier release on
  shutdown or input failure
- optional daemon sandboxing through Linux process hardening, namespace setup,
  and hardened systemd user units for observe-only deployments
- memory, including the semantic desktop graph that ingests `DesktopState`
  snapshots into window, element, and relationship nodes, with optional
  SQLite-backed persistence for values, graph snapshots, desktop events, and
  semantic graph invalidations; fresh graph snapshots also serve as the first
  semantic lookup cache for element and selector-click flows
- AI provider integration

The shared contract lives in `proto/peekaboox/v1/peekaboox.proto`.
