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

from peekaboox.agent import AgentRuntime  # noqa: E402
from peekaboox.client import MissingGrpcDependencyError, Rect  # noqa: E402


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


def start_daemon(out_dir: Path) -> tuple[subprocess.Popen[bytes], str]:
    socket_path = out_dir / "peekabooxd.sock"
    grpc_addr = os.environ.get("PEEKABOOX_CAPTURE_BACKENDS_GRPC_ADDR") or pick_free_grpc_addr()
    audit_log = out_dir / "peekabooxd-audit.jsonl"
    daemon_log = out_dir / "peekabooxd.log"
    command = [
        *daemon_command(),
        "run",
        "--profile",
        "observe",
        "--socket",
        str(socket_path),
        "--grpc-addr",
        grpc_addr,
        "--audit-log",
        str(audit_log),
        "--no-emergency-hotkey",
    ]
    log_handle = daemon_log.open("wb")
    process = subprocess.Popen(command, cwd=ROOT, stdout=log_handle, stderr=subprocess.STDOUT)
    log_handle.close()
    wait_for_socket(socket_path, process, daemon_log)
    wait_for_grpc(grpc_addr, process, daemon_log)
    return process, grpc_addr


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


def assert_discovery(result: Any) -> None:
    if not result.image_backends:
        raise AssertionError("capture backend discovery returned no image backends")
    usable = [
        backend.name
        for backend in result.image_backends
        if backend.available and backend.supports_output
    ]
    if not usable:
        raise AssertionError("no usable output-capable image backend reported")
    if not result.output_path:
        raise AssertionError("capture backend result did not echo output_path")


def assert_probe(result: Any, probe_name: str, region: Rect | None = None) -> None:
    probes = [probe for probe in result.probes if probe.probe == probe_name]
    if not probes:
        raise AssertionError(f"missing probe result: {probe_name}")
    probe = probes[0]
    if not probe.ok:
        raise AssertionError(f"{probe_name} probe failed: {probe.detail}")
    if probe_name == "file" and not probe.output_path:
        raise AssertionError("file probe did not report output_path")
    if region is not None:
        if result.region != region:
            raise AssertionError(f"region mismatch: {result.region} != {region}")
        if probe.width != region.width or probe.height != region.height:
            raise AssertionError(f"region probe size mismatch: {probe.width}x{probe.height}")


def main() -> int:
    run_id = os.environ.get("PEEKABOOX_CAPTURE_BACKENDS_RUN_ID", time.strftime("%Y%m%d-%H%M%S"))
    out_root = Path(
        os.environ.get("PEEKABOOX_EXAMPLE_OUT", ROOT / "target/examples/python-capture-backends")
    )
    out_dir = out_root / run_id
    if out_dir.exists():
        raise SystemExit(f"output directory already exists: {out_dir}")
    out_dir.mkdir(parents=True)

    region = Rect(x=0, y=0, width=320, height=180)
    process: subprocess.Popen[bytes] | None = None
    try:
        process, grpc_addr = start_daemon(out_dir)
        runtime = AgentRuntime.connect(
            grpc_addr,
            capability_profile="observe",
            audit_log_path=out_dir / "runtime-audit.jsonl",
        )

        discovery = runtime.capture_backends(
            output=out_dir / "runtime-screen.png",
            diagnose=True,
        )
        assert_discovery(discovery)
        write_result(out_dir / "backends.json", discovery)

        file_probe = runtime.capture_backends(
            output=out_dir / "probe-file.png",
            diagnose=True,
            probe="file",
        )
        assert_probe(file_probe, "file")
        write_result(out_dir / "probe-file.json", file_probe)

        frame_probe = runtime.capture_backends(
            output=out_dir / "probe-frame.png",
            diagnose=True,
            probe="frame",
        )
        assert_probe(frame_probe, "frame")
        write_result(out_dir / "probe-frame.json", frame_probe)

        region_probe = runtime.capture_backends(
            output=out_dir / "probe-region.png",
            region=region,
            diagnose=True,
            probe="region",
        )
        assert_probe(region_probe, "region", region)
        write_result(out_dir / "probe-region.json", region_probe)

        close = getattr(runtime.client, "close", None)
        if close is not None:
            close()

        summary = {
            "grpc_addr": grpc_addr,
            "out_dir": str(out_dir),
            "session_type": discovery.session_type,
            "desktop": discovery.desktop,
            "image_backends": [
                backend.name
                for backend in discovery.image_backends
                if backend.available and backend.supports_output
            ],
            "probes": [
                probe.probe
                for probe in file_probe.probes + frame_probe.probes + region_probe.probes
            ],
            "region": [region.x, region.y, region.width, region.height],
        }
        print(json.dumps(summary, sort_keys=True))
        return 0
    except MissingGrpcDependencyError as error:
        raise SystemExit(
            f"{error}; run with a Python environment that has the package installed, "
            "or set PEEKABOOX_PYTHON_BIN when using the shell live examples"
        ) from error
    finally:
        if process is not None:
            stop_daemon(process)


if __name__ == "__main__":
    raise SystemExit(main())
