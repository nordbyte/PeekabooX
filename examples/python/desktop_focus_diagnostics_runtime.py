#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import sys
import time
from dataclasses import fields, is_dataclass
from pathlib import Path
from typing import Any


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


ROOT = repo_root()
sys.path.insert(0, str(ROOT / "python" / "src"))

if os.environ.get("PEEKABOOX_PYTHON_BIN") and Path(sys.executable).absolute() != Path(
    os.environ["PEEKABOOX_PYTHON_BIN"]
).absolute():
    os.execv(os.environ["PEEKABOOX_PYTHON_BIN"], [os.environ["PEEKABOOX_PYTHON_BIN"], *sys.argv])

if not os.environ.get("PEEKABOOX_PYTHON_BIN"):
    venv_python = ROOT / ".venv" / "bin" / "python"
    if venv_python.exists() and Path(sys.executable).absolute() != venv_python.absolute():
        os.execv(str(venv_python), [str(venv_python), *sys.argv])

from peekaboox.agent import AgentRuntime  # noqa: E402
from peekaboox.client import DesktopActionResult, MissingGrpcDependencyError  # noqa: E402


def env_bool(name: str, default: bool) -> bool:
    raw = os.environ.get(name)
    if raw is None:
        return default
    value = raw.strip().lower()
    if value in {"1", "true", "yes", "on"}:
        return True
    if value in {"0", "false", "no", "off"}:
        return False
    raise SystemExit(f"{name} must be a boolean value, got {raw!r}")


def env_int(name: str, default: int) -> int:
    raw = os.environ.get(name)
    if raw is None:
        return default
    try:
        value = int(raw)
    except ValueError as error:
        raise SystemExit(f"{name} must be an integer, got {raw!r}") from error
    if value < 0:
        raise SystemExit(f"{name} must be greater than or equal to zero")
    return value


def env_float(name: str, default: float) -> float:
    raw = os.environ.get(name)
    if raw is None:
        return default
    try:
        value = float(raw)
    except ValueError as error:
        raise SystemExit(f"{name} must be a number, got {raw!r}") from error
    if value <= 0:
        raise SystemExit(f"{name} must be greater than zero")
    return value


def optional_env(name: str) -> str | None:
    value = os.environ.get(name)
    if value is None or not value.strip():
        return None
    return value


def pick_free_grpc_addr() -> str:
    sock = socket.socket()
    sock.bind(("127.0.0.1", 0))
    host, port = sock.getsockname()
    sock.close()
    return f"{host}:{port}"


def daemon_command() -> list[str]:
    if os.environ.get("PEEKABOOXD_BIN"):
        return [os.environ["PEEKABOOXD_BIN"]]
    if shutil.which("cargo"):
        return ["cargo", "run", "--quiet", "-p", "peekabooxd", "--"]
    if shutil.which("peekabooxd"):
        return ["peekabooxd"]
    raise SystemExit("peekabooxd is unavailable; build the workspace or set PEEKABOOXD_BIN")


def read_log(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def wait_for_socket(socket_path: Path, process: subprocess.Popen[bytes], log_path: Path) -> None:
    for _ in range(80):
        if socket_path.is_socket():
            return
        if process.poll() is not None:
            raise SystemExit(
                f"peekabooxd exited before creating {socket_path}\n{read_log(log_path)}"
            )
        time.sleep(0.1)
    raise SystemExit(f"timed out waiting for daemon socket: {socket_path}\n{read_log(log_path)}")


def wait_for_grpc(target: str, process: subprocess.Popen[bytes], log_path: Path) -> None:
    host, port_text = target.rsplit(":", 1)
    port = int(port_text)
    for _ in range(80):
        try:
            with socket.create_connection((host, port), timeout=0.2):
                return
        except OSError:
            if process.poll() is not None:
                raise SystemExit(f"peekabooxd exited before opening gRPC\n{read_log(log_path)}")
            time.sleep(0.1)
    raise SystemExit(f"timed out waiting for gRPC: {target}\n{read_log(log_path)}")


def start_daemon(out_dir: Path, grpc_addr: str | None) -> tuple[subprocess.Popen[bytes], str]:
    socket_path = out_dir / "peekabooxd.sock"
    target = grpc_addr or pick_free_grpc_addr()
    audit_log = out_dir / "peekabooxd-audit.jsonl"
    daemon_log = out_dir / "peekabooxd.log"
    command = [
        *daemon_command(),
        "run",
        "--profile",
        "operator",
        "--socket",
        str(socket_path),
        "--grpc-addr",
        target,
        "--audit-log",
        str(audit_log),
        "--no-emergency-hotkey",
    ]
    log_handle = daemon_log.open("wb")
    process = subprocess.Popen(command, cwd=ROOT, stdout=log_handle, stderr=subprocess.STDOUT)
    log_handle.close()
    wait_for_socket(socket_path, process, daemon_log)
    wait_for_grpc(target, process, daemon_log)
    return process, target


def stop_daemon(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


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


def write_result(path: Path, value: Any) -> None:
    path.write_text(json.dumps(json_value(value), indent=2, sort_keys=True) + "\n", encoding="utf-8")


def assert_focus_result(result: DesktopActionResult, expected_app: str) -> None:
    if result.app != expected_app:
        raise AssertionError(f"app mismatch: {result.app!r} != {expected_app!r}")
    if result.action != "focus":
        raise AssertionError(f"unexpected desktop action: {result.action!r}")
    if not result.detail:
        raise AssertionError("desktop_focus returned an empty detail")
    if not result.backend_name:
        raise AssertionError("desktop_focus did not report a backend_name")
    if result.verified is not True:
        raise AssertionError(f"desktop_focus verification failed: {result.verification_detail}")
    if not result.verification_detail:
        raise AssertionError("verified desktop_focus should include verification_detail")
    if not result.focus_diagnostics:
        raise AssertionError("desktop_focus returned no focus_diagnostics")
    if not all(isinstance(item, str) and item for item in result.focus_diagnostics):
        raise AssertionError("focus_diagnostics must contain non-empty strings")
    if not any(item.startswith("verify:") for item in result.focus_diagnostics):
        raise AssertionError("verified desktop_focus should include a verify diagnostic")


def main() -> int:
    run_id = os.environ.get("PEEKABOOX_DESKTOP_FOCUS_RUN_ID", time.strftime("%Y%m%d-%H%M%S"))
    out_root = Path(
        os.environ.get("PEEKABOOX_EXAMPLE_OUT", ROOT / "target/examples/python-desktop-focus")
    )
    out_dir = out_root / run_id
    if out_dir.exists():
        raise SystemExit(f"output directory already exists: {out_dir}")
    out_dir.mkdir(parents=True)

    app = os.environ.get("PEEKABOOX_DESKTOP_FOCUS_APP", "text-editor")
    window_title = optional_env("PEEKABOOX_DESKTOP_FOCUS_WINDOW_TITLE")
    window_id = optional_env("PEEKABOOX_DESKTOP_FOCUS_WINDOW_ID")
    if window_title is not None and window_id is not None:
        raise SystemExit("set either PEEKABOOX_DESKTOP_FOCUS_WINDOW_TITLE or WINDOW_ID, not both")

    start_local_daemon = env_bool("PEEKABOOX_DESKTOP_FOCUS_START_DAEMON", True)
    requested_grpc_addr = optional_env("PEEKABOOX_DESKTOP_FOCUS_GRPC_ADDR")
    grpc_addr = requested_grpc_addr or "127.0.0.1:47777"
    wait_after_focus_ms = env_int("PEEKABOOX_DESKTOP_FOCUS_WAIT_MS", 500)
    overview_wait_ms = env_int("PEEKABOOX_DESKTOP_FOCUS_OVERVIEW_WAIT_MS", 1_000)
    grpc_timeout = env_float("PEEKABOOX_DESKTOP_FOCUS_GRPC_TIMEOUT", 20.0)

    process: subprocess.Popen[bytes] | None = None
    runtime: AgentRuntime | None = None
    try:
        if start_local_daemon:
            process, grpc_addr = start_daemon(out_dir, requested_grpc_addr)

        runtime = AgentRuntime.connect(
            grpc_addr,
            capability_profile="operator",
            audit_log_path=out_dir / "runtime-audit.jsonl",
            client_timeout_seconds=grpc_timeout,
        )
        result = runtime.desktop_focus(
            app,
            use_gnome_overview=env_bool("PEEKABOOX_DESKTOP_FOCUS_USE_GNOME_OVERVIEW", True),
            launch_if_needed=env_bool("PEEKABOOX_DESKTOP_FOCUS_LAUNCH_IF_NEEDED", True),
            wait_after_focus_ms=wait_after_focus_ms,
            overview_wait_ms=overview_wait_ms,
            window_title=window_title,
            window_id=window_id,
            verify=True,
        )
        assert_focus_result(result, app)
        write_result(out_dir / "desktop-focus.json", result)

        summary = {
            "app": result.app,
            "action": result.action,
            "backend_name": result.backend_name,
            "diagnostic_count": len(result.focus_diagnostics),
            "grpc_addr": grpc_addr,
            "last_diagnostic": result.focus_diagnostics[-1],
            "out_dir": str(out_dir),
            "started_daemon": start_local_daemon,
            "verified": result.verified,
        }
        print(json.dumps(summary, sort_keys=True))
        return 0
    except MissingGrpcDependencyError as error:
        raise SystemExit(
            f"{error}; run with a Python environment that has the package installed, "
            "or set PEEKABOOX_PYTHON_BIN"
        ) from error
    finally:
        if runtime is not None and runtime.client is not None:
            close = getattr(runtime.client, "close", None)
            if close is not None:
                close()
        if process is not None:
            stop_daemon(process)


if __name__ == "__main__":
    raise SystemExit(main())
