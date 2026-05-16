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
    category: str = ""
    severity: str = ""

    def __post_init__(self) -> None:
        if not isinstance(self.name, str) or not self.name:
            raise ValueError("doctor check name must be a non-empty string")
        if self.status not in {"ok", "warn", "fail"}:
            raise ValueError(f"doctor check {self.name!r} has invalid status")
        if not isinstance(self.detail, str):
            raise ValueError(f"doctor check {self.name!r} detail must be a string")

        category = self.category or _check_category(self.name)
        if not isinstance(category, str) or not category:
            raise ValueError(f"doctor check {self.name!r} category must be a non-empty string")
        object.__setattr__(self, "category", category)

        severity = self.severity or _severity_for_status(self.status)
        if severity != _severity_for_status(self.status):
            raise ValueError(f"doctor check {self.name!r} has invalid severity")
        object.__setattr__(self, "severity", severity)


@dataclass(frozen=True, slots=True)
class DoctorCategory:
    name: str
    status: str
    severity: str
    ok_count: int
    warn_count: int
    fail_count: int
    total_count: int

    def __post_init__(self) -> None:
        if not isinstance(self.name, str) or not self.name:
            raise ValueError("doctor category name must be a non-empty string")
        if self.status not in {"ok", "warn", "fail"}:
            raise ValueError(f"doctor category {self.name!r} has invalid status")
        if self.severity != _severity_for_status(self.status):
            raise ValueError(f"doctor category {self.name!r} has invalid severity")
        for field_name in ("ok_count", "warn_count", "fail_count", "total_count"):
            value = getattr(self, field_name)
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                raise ValueError(f"doctor category {self.name!r} has invalid {field_name}")
        if self.total_count != self.ok_count + self.warn_count + self.fail_count:
            raise ValueError(f"doctor category {self.name!r} count total does not match")
        if self.status != _rollup_status(self.ok_count, self.warn_count, self.fail_count):
            raise ValueError(f"doctor category {self.name!r} status does not match counts")


@dataclass(frozen=True, slots=True)
class DoctorResult:
    status: str
    checks: tuple[DoctorCheck, ...]
    ok_count: int
    warn_count: int
    fail_count: int
    exit_code: int
    strict: bool = False
    categories: tuple[DoctorCategory, ...] = ()

    def __post_init__(self) -> None:
        checks = tuple(self.checks)
        object.__setattr__(self, "checks", checks)
        if not self.categories:
            object.__setattr__(self, "categories", _category_summaries(checks))
        else:
            object.__setattr__(self, "categories", tuple(self.categories))

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
    raw_categories = payload.get("categories")
    if raw_categories is None:
        categories = _category_summaries(checks)
    elif isinstance(raw_categories, list):
        categories = tuple(_doctor_category_from_payload(category) for category in raw_categories)
    else:
        raise DoctorError("doctor JSON categories must be a list")

    return DoctorResult(
        status=status,
        checks=checks,
        ok_count=sum(1 for check in checks if check.status == "ok"),
        warn_count=sum(1 for check in checks if check.status == "warn"),
        fail_count=sum(1 for check in checks if check.status == "fail"),
        exit_code=exit_code,
        strict=strict,
        categories=categories,
    )


def _doctor_check_from_payload(payload: Any) -> DoctorCheck:
    if not isinstance(payload, dict):
        raise DoctorError("doctor check must be an object")
    name = payload.get("name")
    status = payload.get("status")
    detail = payload.get("detail")
    category = payload.get("category")
    severity = payload.get("severity")
    if not isinstance(name, str) or not name:
        raise DoctorError("doctor check name must be a non-empty string")
    if status not in {"ok", "warn", "fail"}:
        raise DoctorError(f"doctor check {name!r} has invalid status")
    if not isinstance(detail, str):
        raise DoctorError(f"doctor check {name!r} detail must be a string")
    if category is None:
        category = _check_category(name)
    elif not isinstance(category, str) or not category:
        raise DoctorError(f"doctor check {name!r} category must be a non-empty string")
    if severity is None:
        severity = _severity_for_status(status)
    elif severity != _severity_for_status(status):
        raise DoctorError(f"doctor check {name!r} has invalid severity")
    return DoctorCheck(
        name=name,
        status=status,
        detail=detail,
        category=category,
        severity=severity,
    )


def _doctor_category_from_payload(payload: Any) -> DoctorCategory:
    if not isinstance(payload, dict):
        raise DoctorError("doctor category must be an object")
    name = payload.get("name")
    status = payload.get("status")
    severity = payload.get("severity")
    if not isinstance(name, str) or not name:
        raise DoctorError("doctor category name must be a non-empty string")
    if status not in {"ok", "warn", "fail"}:
        raise DoctorError(f"doctor category {name!r} has invalid status")
    if severity != _severity_for_status(status):
        raise DoctorError(f"doctor category {name!r} has invalid severity")
    counts = {}
    for key in ("ok_count", "warn_count", "fail_count", "total_count"):
        value = payload.get(key)
        if not isinstance(value, int) or isinstance(value, bool) or value < 0:
            raise DoctorError(f"doctor category {name!r} missing integer {key}")
        counts[key] = value
    if counts["total_count"] != counts["ok_count"] + counts["warn_count"] + counts["fail_count"]:
        raise DoctorError(f"doctor category {name!r} count total does not match")
    if status != _rollup_status(counts["ok_count"], counts["warn_count"], counts["fail_count"]):
        raise DoctorError(f"doctor category {name!r} status does not match counts")
    return DoctorCategory(
        name=name,
        status=status,
        severity=severity,
        ok_count=counts["ok_count"],
        warn_count=counts["warn_count"],
        fail_count=counts["fail_count"],
        total_count=counts["total_count"],
    )


def _category_summaries(checks: tuple[DoctorCheck, ...]) -> tuple[DoctorCategory, ...]:
    categories = sorted({check.category for check in checks})
    summaries = []
    for category in categories:
        grouped = [check for check in checks if check.category == category]
        ok_count = sum(1 for check in grouped if check.status == "ok")
        warn_count = sum(1 for check in grouped if check.status == "warn")
        fail_count = sum(1 for check in grouped if check.status == "fail")
        status = _rollup_status(ok_count, warn_count, fail_count)
        summaries.append(
            DoctorCategory(
                name=category,
                status=status,
                severity=_severity_for_status(status),
                ok_count=ok_count,
                warn_count=warn_count,
                fail_count=fail_count,
                total_count=len(grouped),
            )
        )
    return tuple(summaries)


def _rollup_status(ok_count: int, warn_count: int, fail_count: int) -> str:
    if fail_count > 0:
        return "fail"
    if warn_count > 0:
        return "warn"
    return "ok"


def _severity_for_status(status: str) -> str:
    if status == "ok":
        return "info"
    if status == "warn":
        return "warning"
    if status == "fail":
        return "error"
    raise ValueError(f"invalid doctor status: {status}")


def _check_category(name: str) -> str:
    if name.startswith("capture-"):
        return "capture"
    if name.startswith("input-"):
        return "input"
    if name in {
        "desktop-session",
        "display-server",
        "windows",
        "desktop-profiles",
        "command:gdbus",
        "command:gtk-launch",
    }:
        return "desktop"
    if name in {"ocr", "command:tesseract"}:
        return "ocr"
    if name in {"python-grpc", "command:python3"}:
        return "python"
    if name in {
        "command:xdotool",
        "command:ydotool",
        "command:wtype",
        "command:wl-copy",
        "command:xclip",
        "command:xsel",
    }:
        return "input"
    return "general"


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
