from __future__ import annotations

import json
import time
from dataclasses import fields, is_dataclass
from pathlib import Path
from typing import Any

from peekaboox.workflows.io import save_workflow_file, workflow_to_dict
from peekaboox.workflows.model import Workflow, workflow_json_schema


def create_workflow_bundle(
    workflow: Workflow,
    output_dir: str | Path,
    *,
    source_path: str | Path | None = None,
    doctor_result: Any | None = None,
    replay_result: Any | None = None,
) -> Path:
    """Write a portable workflow replay bundle and return its directory."""

    bundle_dir = Path(output_dir)
    bundle_dir.mkdir(parents=True, exist_ok=True)
    save_workflow_file(workflow, bundle_dir / "workflow.json", format_name="json")
    save_workflow_file(workflow, bundle_dir / "workflow.yaml", format_name="yaml")
    _write_json(
        bundle_dir / "metadata.json",
        {
            "schema_version": 1,
            "created_at_unix_ms": int(time.time() * 1000),
            "workflow": workflow.name,
            "workflow_schema_version": workflow.schema_version,
            "steps": len(workflow.steps),
            "source_path": None if source_path is None else str(source_path),
        },
    )
    _write_json(bundle_dir / "workflow.normalized.json", workflow_to_dict(workflow))
    _write_json(bundle_dir / "workflow.schema.json", workflow_json_schema())
    if doctor_result is not None:
        _write_json(bundle_dir / "doctor.json", _to_json_value(doctor_result))
    if replay_result is not None:
        _write_json(bundle_dir / "replay-result.json", _to_json_value(replay_result))
    return bundle_dir


def _write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _to_json_value(value: Any) -> Any:
    if is_dataclass(value):
        return {
            field.name: _to_json_value(getattr(value, field.name))
            for field in fields(value)
        }
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    if isinstance(value, tuple | list):
        return [_to_json_value(item) for item in value]
    if isinstance(value, dict):
        return {str(key): _to_json_value(item) for key, item in value.items()}
    if isinstance(value, Path):
        return str(value)
    return value
