from __future__ import annotations

import json
import os
import shutil
import subprocess
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any


DEFAULT_DOCTOR_TIMEOUT_SECONDS = 30.0


class DoctorError(RuntimeError):
    """Raised when the PeekabooX doctor command cannot be executed or parsed."""


@dataclass(frozen=True, slots=True)
class DoctorCheck:
    name: str
    status: str
    detail: str


@dataclass(frozen=True, slots=True)
class DoctorResult:
    status: str
    checks: tuple[DoctorCheck, ...]
    ok_count: int
    warn_count: int
    fail_count: int
    exit_code: int
    strict: bool = False

    @property
    def ok(self) -> bool:
        return self.status == "ok" and self.fail_count == 0


def run_doctor(
    *,
    strict: bool = False,
    timeout_seconds: float = DEFAULT_DOCTOR_TIMEOUT_SECONDS,
    command: Sequence[str] | None = None,
) -> DoctorResult:
    """Run `peekaboox doctor --json` and return the structured result."""

    doctor_command = list(command) if command is not None else _doctor_command()
    if not doctor_command:
        raise DoctorError("doctor command must not be empty")
    args = [*doctor_command, "doctor", "--json"]
    if strict:
        args.append("--strict")

    try:
        completed = subprocess.run(
            args,
            cwd=_repo_root() or None,
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout_seconds,
        )
    except FileNotFoundError as error:
        raise DoctorError(f"doctor command not found: {doctor_command[0]}") from error
    except subprocess.TimeoutExpired as error:
        raise DoctorError(f"doctor command timed out after {timeout_seconds:g}s") from error

    if not completed.stdout.strip():
        detail = completed.stderr.strip() or f"exit code {completed.returncode}"
        raise DoctorError(f"doctor command did not return JSON: {detail}")

    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise DoctorError(f"doctor command returned invalid JSON: {error.msg}") from error

    result = _doctor_result_from_payload(payload, completed.returncode, strict)
    if completed.returncode != 0 and not (strict and result.status == "fail"):
        detail = completed.stderr.strip() or f"exit code {completed.returncode}"
        raise DoctorError(f"doctor command failed: {detail}")
    return result


def _doctor_result_from_payload(payload: Any, exit_code: int, strict: bool) -> DoctorResult:
    if not isinstance(payload, dict):
        raise DoctorError("doctor JSON payload must be an object")
    status = payload.get("status")
    if status not in {"ok", "fail"}:
        raise DoctorError("doctor JSON status must be 'ok' or 'fail'")
    raw_checks = payload.get("checks")
    if not isinstance(raw_checks, list):
        raise DoctorError("doctor JSON checks must be a list")

    checks = tuple(_doctor_check_from_payload(check) for check in raw_checks)
    return DoctorResult(
        status=status,
        checks=checks,
        ok_count=sum(1 for check in checks if check.status == "ok"),
        warn_count=sum(1 for check in checks if check.status == "warn"),
        fail_count=sum(1 for check in checks if check.status == "fail"),
        exit_code=exit_code,
        strict=strict,
    )


def _doctor_check_from_payload(payload: Any) -> DoctorCheck:
    if not isinstance(payload, dict):
        raise DoctorError("doctor check must be an object")
    name = payload.get("name")
    status = payload.get("status")
    detail = payload.get("detail")
    if not isinstance(name, str) or not name:
        raise DoctorError("doctor check name must be a non-empty string")
    if status not in {"ok", "warn", "fail"}:
        raise DoctorError(f"doctor check {name!r} has invalid status")
    if not isinstance(detail, str):
        raise DoctorError(f"doctor check {name!r} detail must be a string")
    return DoctorCheck(name=name, status=status, detail=detail)


def _doctor_command() -> list[str]:
    if os.environ.get("PEEKABOOX_BIN"):
        return [os.environ["PEEKABOOX_BIN"]]

    root = _repo_root()
    if root is not None and shutil.which("cargo"):
        return ["cargo", "run", "--quiet", "-p", "peekaboox-cli", "--"]

    debug_binary = root / "target" / "debug" / "peekaboox" if root is not None else None
    if debug_binary is not None and debug_binary.is_file() and os.access(debug_binary, os.X_OK):
        return [str(debug_binary)]

    if shutil.which("peekaboox"):
        return ["peekaboox"]
    return ["peekaboox"]


def _repo_root() -> Path | None:
    for parent in Path(__file__).resolve().parents:
        if (parent / "Cargo.toml").is_file() and (parent / "proto").is_dir():
            return parent
    return None
