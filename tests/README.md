# Test Strategy

Planned test layers:

- Rust unit tests per crate.
- Python unit tests for agent, planning, workflows, MCP, and memory.
- Desktop integration tests in controlled VM sessions.
- Screenshot and semantic-tree regression fixtures.
- API surface parity checks under `tests/api_surface_check.py` for CLI,
  protobuf RPCs, Python client methods, MCP tools, and command docs.
- Benchmark regression checks under `benchmarks/perf_baseline.py` for workflow
  parsing, semantic graph ingest/query, cached agent cycles, MCP tool metadata,
  and audit logging.
