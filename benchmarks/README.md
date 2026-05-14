# Benchmarks

Release benchmark targets:

- Capture latency: below 20ms where the backend supports it.
- Input latency: below 5ms where direct injection is available.
- Semantic query latency: below 50ms with cache.
- Agent action cycle: below 500ms for cached semantic operations.

## Deterministic Regression Harness

Run the CI-safe Python benchmark suite from the repository root:

```bash
PYTHONPATH=python/src python3 benchmarks/perf_baseline.py
```

The harness uses local fixtures and fake runtime clients, so it does not require
a live desktop, gRPC daemon, or input backend. It measures deterministic hot
paths for workflow parsing, semantic graph ingest/query, cached agent click
cycles, MCP tool listing, and JSONL audit logging.

Budgets live in `benchmarks/perf_budgets.json`. Results are written to
`target/benchmark-results.json` by default and the process exits non-zero when a
case exceeds its p95 latency budget.

Useful options:

```bash
python3 benchmarks/perf_baseline.py --list
python3 benchmarks/perf_baseline.py --iterations 30 --json-out target/perf-smoke.json
python3 benchmarks/perf_baseline.py --case python.memory.cached_selector_query
```
