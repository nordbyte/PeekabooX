#!/usr/bin/env python3
from __future__ import annotations

import base64
import json
import os
import shutil
import socket
import subprocess
import sys
import time
from dataclasses import fields, is_dataclass
from pathlib import Path
from tempfile import TemporaryDirectory
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


def read_log(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def start_daemon(out_dir: Path) -> tuple[subprocess.Popen[bytes], str]:
    socket_path = out_dir / "peekabooxd.sock"
    grpc_addr = os.environ.get("PEEKABOOX_CAPTURE_DELTA_GRPC_ADDR") or pick_free_grpc_addr()
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
    if isinstance(value, bytes):
        return base64.b64encode(value).decode("ascii")
    if isinstance(value, tuple | list):
        return [json_value(item) for item in value]
    if isinstance(value, dict):
        return {str(key): json_value(item) for key, item in value.items()}
    return value


def write_result(path: Path, value: Any) -> None:
    path.write_text(json.dumps(json_value(value), indent=2, sort_keys=True) + "\n", encoding="utf-8")


def assert_delta(
    result: Any,
    *,
    stream_id: str,
    sequence: int,
    full_frame: bool,
    low_bandwidth: bool,
    region: Rect | None = None,
) -> None:
    if result.stream_id != stream_id:
        raise AssertionError(f"stream_id mismatch: {result.stream_id!r} != {stream_id!r}")
    if result.sequence != sequence:
        raise AssertionError(f"sequence mismatch: {result.sequence} != {sequence}")
    if result.full_frame is not full_frame:
        raise AssertionError(f"full_frame mismatch: {result.full_frame} != {full_frame}")
    if result.low_bandwidth is not low_bandwidth:
        raise AssertionError(f"low_bandwidth mismatch: {result.low_bandwidth} != {low_bandwidth}")
    if result.frame_width <= 0 or result.frame_height <= 0:
        raise AssertionError("frame dimensions must be positive")
    if result.metadata is None or not result.metadata.backend:
        raise AssertionError("capture metadata backend is missing")
    if full_frame and not result.patch:
        raise AssertionError("full-frame capture delta must include patch bytes")
    if region is None:
        if result.capture_region is not None:
            raise AssertionError(f"unexpected capture region: {result.capture_region}")
    else:
        if result.capture_region != region:
            raise AssertionError(f"capture region mismatch: {result.capture_region} != {region}")
        if result.frame_width != region.width or result.frame_height != region.height:
            raise AssertionError(
                f"region frame size mismatch: {result.frame_width}x{result.frame_height}"
            )


def main() -> int:
    run_id = os.environ.get("PEEKABOOX_CAPTURE_DELTA_RUN_ID", time.strftime("%Y%m%d-%H%M%S"))
    out_root = Path(os.environ.get("PEEKABOOX_EXAMPLE_OUT", ROOT / "target/examples/python-capture-delta"))
    out_dir = out_root / run_id
    if out_dir.exists():
        raise SystemExit(f"output directory already exists: {out_dir}")
    out_dir.mkdir(parents=True)

    region = Rect(x=0, y=0, width=320, height=180)
    primary_stream = f"python-runtime-{run_id}-primary"
    region_stream = f"python-runtime-{run_id}-region"

    process: subprocess.Popen[bytes] | None = None
    try:
        process, grpc_addr = start_daemon(out_dir)
        runtime = AgentRuntime.connect(
            grpc_addr,
            capability_profile="observe",
            audit_log_path=out_dir / "runtime-audit.jsonl",
        )

        first = runtime.capture_delta(stream_id=primary_stream, reset=True, low_bandwidth=True)
        assert_delta(first, stream_id=primary_stream, sequence=1, full_frame=True, low_bandwidth=True)
        write_result(out_dir / "primary-reset.json", first)

        second = runtime.capture_delta(stream_id=primary_stream, low_bandwidth=True)
        assert_delta(
            second,
            stream_id=primary_stream,
            sequence=2,
            full_frame=False,
            low_bandwidth=True,
        )
        write_result(out_dir / "primary-delta.json", second)

        forced = runtime.capture_delta(stream_id=primary_stream, low_bandwidth=False)
        assert_delta(
            forced,
            stream_id=primary_stream,
            sequence=3,
            full_frame=True,
            low_bandwidth=False,
        )
        write_result(out_dir / "primary-forced-full.json", forced)

        region_first = runtime.capture_delta(
            stream_id=region_stream,
            reset=True,
            region=region,
            per_channel_threshold=1,
        )
        assert_delta(
            region_first,
            stream_id=region_stream,
            sequence=1,
            full_frame=True,
            low_bandwidth=True,
            region=region,
        )
        write_result(out_dir / "region-reset.json", region_first)

        region_second = runtime.capture_delta(stream_id=region_stream, region=region)
        assert_delta(
            region_second,
            stream_id=region_stream,
            sequence=2,
            full_frame=False,
            low_bandwidth=True,
            region=region,
        )
        write_result(out_dir / "region-delta.json", region_second)

        close = getattr(runtime.client, "close", None)
        if close is not None:
            close()

        summary = {
            "grpc_addr": grpc_addr,
            "out_dir": str(out_dir),
            "primary": {
                "stream_id": primary_stream,
                "sequences": [first.sequence, second.sequence, forced.sequence],
                "forced_full_frame": forced.full_frame,
            },
            "region": {
                "stream_id": region_stream,
                "sequences": [region_first.sequence, region_second.sequence],
                "size": [region_second.frame_width, region_second.frame_height],
            },
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
