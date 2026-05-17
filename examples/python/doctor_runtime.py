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
from peekaboox.security import CapabilityPolicy, CapabilityProfile  # noqa: E402


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
    if not result.categories:
        raise AssertionError("doctor returned no category summaries")
    names = {check.name for check in result.checks}
    required = {"desktop-session", "display-server", "desktop-profiles"}
    missing = sorted(required - names)
    if missing:
        raise AssertionError(f"doctor output is missing checks: {', '.join(missing)}")
    for check in result.checks:
        if check.status not in {"ok", "warn", "fail"}:
            raise AssertionError(f"doctor check {check.name} has invalid status: {check.status}")
        if check.severity not in {"info", "warning", "error"}:
            raise AssertionError(
                f"doctor check {check.name} has invalid severity: {check.severity}"
            )
        if not check.category:
            raise AssertionError(f"doctor check {check.name} is missing a category")
    categories = {category.name: category for category in result.categories}
    required_categories = {"desktop", "capture", "input", "ocr", "python"}
    missing_categories = sorted(required_categories - categories.keys())
    if missing_categories:
        raise AssertionError(
            f"doctor output is missing categories: {', '.join(missing_categories)}"
        )
    for category in result.categories:
        if category.total_count != category.ok_count + category.warn_count + category.fail_count:
            raise AssertionError(f"doctor category {category.name} count total does not match")

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
        "categories": {name: category.status for name, category in categories.items()},
    }
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
