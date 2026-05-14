# Test Strategy

Planned test layers:

- Rust unit tests per crate.
- Python unit tests for agent, planning, workflows, MCP, and memory.
- Desktop integration tests in controlled VM sessions.
- Screenshot and semantic-tree regression fixtures.
- Benchmark regression checks under `benchmarks/perf_baseline.py` for workflow
  parsing, semantic graph ingest/query, cached agent cycles, MCP tool metadata,
  and audit logging.
