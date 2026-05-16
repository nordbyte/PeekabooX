#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import shutil
import shlex
import subprocess
import sys
import time
from pathlib import Path

from desktop_focus_diagnostics_runtime import (  # noqa: E402
    AgentRuntime,
    DesktopActionResult,
    MissingGrpcDependencyError,
    ROOT,
    env_bool,
    env_float,
    env_int,
    optional_env,
    start_daemon,
    stop_daemon,
    write_result,
)


def text_editor_command() -> list[str]:
    custom = optional_env("PEEKABOOX_DESKTOP_ACTIONS_TEXT_EDITOR_COMMAND")
    if custom is not None:
        return shlex.split(custom)
    for candidate in ("gnome-text-editor", "gedit"):
        path = shutil.which(candidate)
        if path is not None:
            return [path]
    raise SystemExit(
        "GNOME Text Editor is unavailable; install gnome-text-editor or set "
        "PEEKABOOX_DESKTOP_ACTIONS_TEXT_EDITOR_COMMAND"
    )


def launch_text_editor(draft_file: Path) -> subprocess.Popen[bytes] | None:
    if not env_bool("PEEKABOOX_DESKTOP_ACTIONS_LAUNCH_EDITOR", True):
        return None
    command = [*text_editor_command(), str(draft_file)]
    return subprocess.Popen(
        command,
        cwd=ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def wait_for_focus(
    runtime: AgentRuntime,
    app: str,
    window_title: str | None,
    window_id: str | None,
    *,
    wait_after_focus_ms: int,
    overview_wait_ms: int,
) -> DesktopActionResult:
    last_error: Exception | None = None
    for _ in range(24):
        try:
            return runtime.desktop_focus(
                app,
                wait_after_focus_ms=wait_after_focus_ms,
                overview_wait_ms=overview_wait_ms,
                window_title=window_title,
                window_id=window_id,
                verify=True,
            )
        except Exception as error:  # noqa: BLE001 - live desktop startup can fail in several layers.
            last_error = error
            time.sleep(0.25)
    raise SystemExit(f"timed out waiting for {app!r} focus target: {last_error}")


def assert_action_result(
    result: DesktopActionResult,
    *,
    expected_app: str,
    expected_action: str,
    require_action_verification: bool,
) -> None:
    if result.app != expected_app:
        raise AssertionError(f"app mismatch: {result.app!r} != {expected_app!r}")
    if result.action != expected_action:
        raise AssertionError(f"action mismatch: {result.action!r} != {expected_action!r}")
    if not result.detail:
        raise AssertionError(f"{expected_action} returned an empty detail")
    if not result.backend_name:
        raise AssertionError(f"{expected_action} did not report a backend_name")
    if require_action_verification and result.verified is not True:
        raise AssertionError(f"{expected_action} verification failed: {result.verification_detail}")
    if require_action_verification and not result.verification_detail:
        raise AssertionError(f"{expected_action} should include verification_detail")
    diagnostics = result.focus_diagnostics
    if not diagnostics:
        raise AssertionError(f"{expected_action} returned no focus_diagnostics")
    if not all(isinstance(item, str) and item for item in diagnostics):
        raise AssertionError(f"{expected_action} focus_diagnostics must contain non-empty strings")
    if not any(item.startswith("verify:") for item in diagnostics):
        raise AssertionError(f"{expected_action} should include a focus verify diagnostic")


def main() -> int:
    run_id = os.environ.get("PEEKABOOX_DESKTOP_ACTIONS_RUN_ID", time.strftime("%Y%m%d-%H%M%S"))
    out_root = Path(
        os.environ.get("PEEKABOOX_EXAMPLE_OUT", ROOT / "target/examples/python-desktop-actions")
    )
    out_dir = out_root / run_id
    if out_dir.exists():
        raise SystemExit(f"output directory already exists: {out_dir}")
    out_dir.mkdir(parents=True)

    app = os.environ.get("PEEKABOOX_DESKTOP_ACTIONS_APP", "text-editor")
    target = os.environ.get("PEEKABOOX_DESKTOP_ACTIONS_TARGET", "document")
    text = os.environ.get(
        "PEEKABOOX_DESKTOP_ACTIONS_TEXT",
        f"PeekabooX desktop action diagnostics {run_id}",
    )
    draft_file = out_dir / f"peekaboox-desktop-actions-{run_id}.txt"
    draft_file.write_text("PeekabooX desktop action diagnostics draft\n", encoding="utf-8")

    window_id = optional_env("PEEKABOOX_DESKTOP_ACTIONS_WINDOW_ID")
    window_title = optional_env("PEEKABOOX_DESKTOP_ACTIONS_WINDOW_TITLE")
    if window_title is None and window_id is None:
        window_title = draft_file.name
    if window_title is not None and window_id is not None:
        raise SystemExit("set either PEEKABOOX_DESKTOP_ACTIONS_WINDOW_TITLE or WINDOW_ID, not both")

    start_local_daemon = env_bool("PEEKABOOX_DESKTOP_ACTIONS_START_DAEMON", True)
    requested_grpc_addr = optional_env("PEEKABOOX_DESKTOP_ACTIONS_GRPC_ADDR")
    grpc_addr = requested_grpc_addr or "127.0.0.1:47777"
    wait_after_focus_ms = env_int("PEEKABOOX_DESKTOP_ACTIONS_WAIT_MS", 500)
    overview_wait_ms = env_int("PEEKABOOX_DESKTOP_ACTIONS_OVERVIEW_WAIT_MS", 1_000)
    grpc_timeout = env_float("PEEKABOOX_DESKTOP_ACTIONS_GRPC_TIMEOUT", 20.0)
    type_verify = env_bool("PEEKABOOX_DESKTOP_ACTIONS_TYPE_VERIFY", False)

    daemon: subprocess.Popen[bytes] | None = None
    editor: subprocess.Popen[bytes] | None = None
    runtime: AgentRuntime | None = None
    try:
        if start_local_daemon:
            daemon, grpc_addr = start_daemon(out_dir, requested_grpc_addr)
        editor = launch_text_editor(draft_file)

        runtime = AgentRuntime.connect(
            grpc_addr,
            capability_profile="operator",
            audit_log_path=out_dir / "runtime-audit.jsonl",
            client_timeout_seconds=grpc_timeout,
        )
        focus = wait_for_focus(
            runtime,
            app,
            window_title,
            window_id,
            wait_after_focus_ms=wait_after_focus_ms,
            overview_wait_ms=overview_wait_ms,
        )
        write_result(out_dir / "focus.json", focus)

        click = runtime.desktop_click(
            app,
            target,
            window_title=window_title,
            window_id=window_id,
            button="left",
            verify=True,
        )
        assert_action_result(
            click,
            expected_app=app,
            expected_action="click",
            require_action_verification=True,
        )
        write_result(out_dir / "desktop-click.json", click)

        drag = runtime.desktop_drag(
            app,
            target,
            window_title=window_title,
            window_id=window_id,
            from_ratio=(0.2, 0.5),
            to_ratio=(0.8, 0.5),
            duration_ms=120,
            verify=True,
        )
        assert_action_result(
            drag,
            expected_app=app,
            expected_action="drag",
            require_action_verification=True,
        )
        write_result(out_dir / "desktop-drag.json", drag)

        typed = runtime.desktop_type_into(
            app,
            target,
            text,
            window_title=window_title,
            window_id=window_id,
            clear=True,
            verify=type_verify,
        )
        assert_action_result(
            typed,
            expected_app=app,
            expected_action="type-into",
            require_action_verification=type_verify,
        )
        write_result(out_dir / "desktop-type-into.json", typed)

        summary = {
            "app": app,
            "target": target,
            "draft_file": str(draft_file),
            "grpc_addr": grpc_addr,
            "out_dir": str(out_dir),
            "started_daemon": start_local_daemon,
            "started_editor": editor is not None,
            "actions": {
                "click": {
                    "verified": click.verified,
                    "diagnostic_count": len(click.focus_diagnostics),
                    "last_diagnostic": click.focus_diagnostics[-1],
                },
                "drag": {
                    "verified": drag.verified,
                    "diagnostic_count": len(drag.focus_diagnostics),
                    "last_diagnostic": drag.focus_diagnostics[-1],
                },
                "type_into": {
                    "verified": typed.verified,
                    "diagnostic_count": len(typed.focus_diagnostics),
                    "last_diagnostic": typed.focus_diagnostics[-1],
                },
            },
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
        if daemon is not None:
            stop_daemon(daemon)
        # The editor window is intentionally left open so users can inspect the result.


if __name__ == "__main__":
    raise SystemExit(main())
