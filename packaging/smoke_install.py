#!/usr/bin/env python3
from __future__ import annotations

import argparse
import glob
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_WHEEL_GLOB = REPO_ROOT / "target" / "python-wheel" / "peekaboox-*.whl"
DEFAULT_DEB_GLOB = REPO_ROOT / "target" / "dist" / "peekaboox_*.deb"
DEFAULT_CARGO_ROOT = REPO_ROOT / "target" / "install-smoke" / "cargo"


def main() -> int:
    parser = argparse.ArgumentParser(description="Smoke-test PeekabooX install artifacts.")
    parser.add_argument("--wheel", default=str(DEFAULT_WHEEL_GLOB))
    parser.add_argument("--deb", default=str(DEFAULT_DEB_GLOB))
    parser.add_argument("--cargo-root", default=str(DEFAULT_CARGO_ROOT))
    parser.add_argument("--skip-wheel", action="store_true")
    parser.add_argument("--skip-deb", action="store_true")
    parser.add_argument("--skip-cargo-install", action="store_true")
    args = parser.parse_args()

    if not args.skip_wheel:
        smoke_wheel(resolve_one(args.wheel))
    if not args.skip_deb:
        smoke_deb(resolve_one(args.deb))
    if not args.skip_cargo_install:
        smoke_cargo_install(Path(args.cargo_root))
    return 0


def resolve_one(pattern: str) -> Path:
    matches = sorted(Path(path) for path in glob.glob(pattern))
    if not matches:
        raise FileNotFoundError(f"no artifact matched {pattern}")
    return matches[-1]


def smoke_wheel(wheel: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="peekaboox-wheel-smoke-") as tmpdir:
        venv = Path(tmpdir) / "venv"
        run([sys.executable, "-m", "venv", str(venv)])
        python = venv / "bin" / "python"
        env = os.environ.copy()
        env["PYTHONNOUSERSITE"] = "1"
        run([str(python), "-m", "pip", "install", "--no-deps", str(wheel)], env=env)
        run([str(python), "-m", "compileall", "-q", str(venv / "lib")], env=env)
        run([str(venv / "bin" / "peekaboox-agent"), "--version"], env=env)
        run(
            [
                str(venv / "bin" / "peekaboox-agent"),
                "plugins",
                "--path",
                str(REPO_ROOT / "examples" / "plugins"),
            ],
            env=env,
        )
        run([str(venv / "bin" / "peekaboox-mcp"), "--list-tools"], env=env)


def smoke_deb(deb: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="peekaboox-deb-smoke-") as tmpdir:
        root = Path(tmpdir) / "root"
        run(["dpkg-deb", "-I", str(deb)])
        run(["dpkg-deb", "-x", str(deb), str(root)])
        peekaboox = root / "usr" / "bin" / "peekaboox"
        peekabooxd = root / "usr" / "bin" / "peekabooxd"
        require_executable(peekaboox)
        require_executable(peekabooxd)
        run([str(peekaboox), "--version"])
        run([str(peekabooxd), "--version"])
        run(
            [
                str(peekaboox),
                "plugins",
                "--path",
                str(root / "usr" / "share" / "peekaboox" / "examples" / "plugins"),
            ]
        )


def smoke_cargo_install(root: Path) -> None:
    if root.exists():
        shutil.rmtree(root)
    run(
        [
            "cargo",
            "install",
            "--locked",
            "--debug",
            "--force",
            "--root",
            str(root),
            "--path",
            str(REPO_ROOT / "cli"),
            "--bin",
            "peekaboox",
        ],
        cwd=REPO_ROOT,
    )
    run(
        [
            "cargo",
            "install",
            "--locked",
            "--debug",
            "--force",
            "--root",
            str(root),
            "--path",
            str(REPO_ROOT / "rust" / "daemon"),
            "--bin",
            "peekabooxd",
        ],
        cwd=REPO_ROOT,
    )
    run([str(root / "bin" / "peekaboox"), "--version"])
    run([str(root / "bin" / "peekabooxd"), "--version"])


def require_executable(path: Path) -> None:
    if not path.is_file() or not os.access(path, os.X_OK):
        raise RuntimeError(f"expected executable artifact at {path}")


def run(
    command: list[str],
    *,
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
) -> None:
    subprocess.run(command, cwd=cwd or REPO_ROOT, env=env, check=True)


if __name__ == "__main__":
    raise SystemExit(main())
