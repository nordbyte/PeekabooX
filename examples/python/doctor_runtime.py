#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
import time
from dataclasses import fields, is_dataclass
from pathlib import Path
from typing import Any


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


ROOT = repo_root()
sys.path.insert(0, str(ROOT / "python" / "src"))

from peekaboox.agent import AgentRuntime  # noqa: E402
from peekaboox.security import CapabilityProfile, CapabilityPolicy  # noqa: E402


def json_value(value: Any) -> Any:
    if is_dataclass(value):
        return {field.name: json_value(getattr(value, field.name)) for field in fields(value)}
    if isinstance(value, tuple | list):
        return [json_value(item) for item in value]
    if isinstance(value, dict):
        return {str(key): json_value(item) for key, item in value.items()}
    if isinstance(value, Path):
        return str(value)
    return value


def main() -> int:
    run_id = os.environ.get("PEEKABOOX_DOCTOR_RUN_ID", time.strftime("%Y%m%d-%H%M%S"))
    out_root = Path(os.environ.get("PEEKABOOX_EXAMPLE_OUT", ROOT / "target/examples/python-doctor"))
    out_dir = out_root / run_id
    if out_dir.exists():
        raise SystemExit(f"output directory already exists: {out_dir}")
    out_dir.mkdir(parents=True)

    runtime = AgentRuntime(
        capability_policy=CapabilityPolicy.from_profile(CapabilityProfile.OBSERVE),
        plugin_paths=(ROOT / "examples" / "plugins",),
    )
    result = runtime.doctor(strict=False)
    if not result.checks:
        raise AssertionError("doctor returned no checks")
    if result.status not in {"ok", "fail"}:
        raise AssertionError(f"unexpected doctor status: {result.status}")
    names = {check.name for check in result.checks}
    required = {"desktop-session", "display-server", "desktop-profiles"}
    missing = sorted(required - names)
    if missing:
        raise AssertionError(f"doctor output is missing checks: {', '.join(missing)}")

    output_path = out_dir / "doctor.json"
    output_path.write_text(
        json.dumps(json_value(result), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    summary = {
        "out_dir": str(out_dir),
        "status": result.status,
        "ok": result.ok_count,
        "warn": result.warn_count,
        "fail": result.fail_count,
        "checks": len(result.checks),
    }
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
