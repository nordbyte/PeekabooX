#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT_DIR = REPO_ROOT / "target" / "dist"
PACKAGE_NAME = "peekaboox"
MAINTAINER = "PeekabooX Maintainers <maintainers@marketdeck.io>"
SECTION = "utils"
PRIORITY = "optional"
DESCRIPTION = "Linux-native AI desktop automation runtime"
DEPENDS = "libc6, libdbus-1-3, libgcc-s1, systemd"
RECOMMENDS = "xdg-desktop-portal, tesseract-ocr, ydotool | wtype | xdotool"


@dataclass(frozen=True, slots=True)
class PackagePlan:
    version: str
    architecture: str
    output_dir: Path
    stage_dir: Path
    deb_path: Path


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a minimal PeekabooX .deb package.")
    parser.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR))
    parser.add_argument("--architecture", default=detect_architecture())
    parser.add_argument(
        "--skip-cargo-build",
        action="store_true",
        help="use existing target/release binaries instead of running cargo build",
    )
    parser.add_argument(
        "--features",
        default=os.environ.get("PEEKABOOX_DEB_FEATURES", ""),
        help="comma-separated Rust features for peekaboox-cli and peekabooxd",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="validate packaging metadata and print the package plan without building",
    )
    args = parser.parse_args()

    plan = package_plan(Path(args.output_dir), args.architecture)
    validate_inputs()

    if args.check:
        print(
            json.dumps(
                {
                    "package": PACKAGE_NAME,
                    "version": plan.version,
                    "architecture": plan.architecture,
                    "deb_path": str(plan.deb_path),
                    "features": args.features,
                    "systemd_units": [
                        "integrations/systemd/peekabooxd.service",
                        "integrations/systemd/peekabooxd-hardened.service",
                    ],
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 0

    if not args.skip_cargo_build:
        build_release_binaries(args.features)

    stage_package(plan)
    build_deb(plan)
    print(plan.deb_path)
    return 0


def package_plan(output_dir: Path, architecture: str) -> PackagePlan:
    version = workspace_version()
    deb_path = output_dir / f"{PACKAGE_NAME}_{version}_{architecture}.deb"
    stage_dir = output_dir / f"{PACKAGE_NAME}_{version}_{architecture}"
    return PackagePlan(
        version=version,
        architecture=architecture,
        output_dir=output_dir,
        stage_dir=stage_dir,
        deb_path=deb_path,
    )


def workspace_version() -> str:
    cargo_toml = (REPO_ROOT / "Cargo.toml").read_text(encoding="utf-8")
    in_workspace_package = False
    for raw_line in cargo_toml.splitlines():
        line = raw_line.strip()
        if line == "[workspace.package]":
            in_workspace_package = True
            continue
        if line.startswith("[") and line.endswith("]"):
            in_workspace_package = False
        if in_workspace_package and line.startswith("version"):
            return line.split("=", 1)[1].strip().strip('"')
    raise RuntimeError("workspace package version not found in Cargo.toml")


def detect_architecture() -> str:
    try:
        return subprocess.check_output(
            ["dpkg", "--print-architecture"],
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return "amd64"


def validate_inputs() -> None:
    required_paths = [
        REPO_ROOT / "cli" / "Cargo.toml",
        REPO_ROOT / "rust" / "daemon" / "Cargo.toml",
        REPO_ROOT / "integrations" / "systemd" / "peekabooxd.service",
        REPO_ROOT / "integrations" / "systemd" / "peekabooxd-hardened.service",
        REPO_ROOT / "CHANGELOG.md",
        REPO_ROOT / "README.md",
        REPO_ROOT / "docs" / "plugins.md",
        REPO_ROOT / "docs" / "release.md",
        REPO_ROOT / "examples" / "plugins" / "system-info" / "peekaboox.plugin.json",
    ]
    missing = [path for path in required_paths if not path.exists()]
    if missing:
        details = ", ".join(str(path.relative_to(REPO_ROOT)) for path in missing)
        raise FileNotFoundError(f"missing packaging inputs: {details}")


def build_release_binaries(features: str) -> None:
    command = ["cargo", "build", "--release", "-p", "peekaboox-cli", "-p", "peekabooxd"]
    if features:
        command.extend(["--features", features])
    subprocess.run(command, cwd=REPO_ROOT, check=True)


def stage_package(plan: PackagePlan) -> None:
    if plan.stage_dir.exists():
        shutil.rmtree(plan.stage_dir)
    if plan.deb_path.exists():
        plan.deb_path.unlink()

    binary_dir = plan.stage_dir / "usr" / "bin"
    doc_dir = plan.stage_dir / "usr" / "share" / "doc" / PACKAGE_NAME
    example_dir = plan.stage_dir / "usr" / "share" / PACKAGE_NAME / "examples"
    systemd_dir = plan.stage_dir / "usr" / "lib" / "systemd" / "user"
    control_dir = plan.stage_dir / "DEBIAN"

    for directory in (binary_dir, doc_dir, example_dir, systemd_dir, control_dir):
        directory.mkdir(parents=True, exist_ok=True)

    copy_executable(REPO_ROOT / "target" / "release" / "peekaboox", binary_dir / "peekaboox")
    copy_executable(REPO_ROOT / "target" / "release" / "peekabooxd", binary_dir / "peekabooxd")
    copy_file(REPO_ROOT / "README.md", doc_dir / "README.md")
    copy_file(REPO_ROOT / "CHANGELOG.md", doc_dir / "CHANGELOG.md")
    for doc_name in ("api.md", "architecture.md", "plugins.md", "release.md", "security.md"):
        copy_file(REPO_ROOT / "docs" / doc_name, doc_dir / doc_name)
    copy_file(REPO_ROOT / "examples" / "workflow.yaml", example_dir / "workflow.yaml")
    copy_tree(REPO_ROOT / "examples" / "plugins", example_dir / "plugins")
    copy_file(
        REPO_ROOT / "integrations" / "systemd" / "peekabooxd.service",
        systemd_dir / "peekabooxd.service",
    )
    copy_file(
        REPO_ROOT / "integrations" / "systemd" / "peekabooxd-hardened.service",
        systemd_dir / "peekabooxd-hardened.service",
    )
    write_control_file(plan, control_dir / "control")
    normalize_permissions(plan.stage_dir)


def copy_executable(source: Path, destination: Path) -> None:
    if not source.exists():
        raise FileNotFoundError(f"release binary not found: {source}")
    shutil.copy2(source, destination)
    destination.chmod(0o755)


def copy_file(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)


def copy_tree(source: Path, destination: Path) -> None:
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(source, destination)


def write_control_file(plan: PackagePlan, path: Path) -> None:
    installed_size = installed_size_kib(plan.stage_dir)
    path.write_text(
        "\n".join(
            [
                f"Package: {PACKAGE_NAME}",
                f"Version: {plan.version}",
                f"Architecture: {plan.architecture}",
                f"Maintainer: {MAINTAINER}",
                f"Section: {SECTION}",
                f"Priority: {PRIORITY}",
                f"Installed-Size: {installed_size}",
                f"Depends: {DEPENDS}",
                f"Recommends: {RECOMMENDS}",
                f"Description: {DESCRIPTION}",
                " PeekabooX provides a local Linux desktop automation daemon,",
                " CLI, systemd user units, plugin examples, and documentation.",
                "",
            ]
        ),
        encoding="utf-8",
    )


def installed_size_kib(root: Path) -> int:
    total = 0
    for path in root.rglob("*"):
        if path.is_file() and "DEBIAN" not in path.parts:
            total += path.stat().st_size
    return max(1, (total + 1023) // 1024)


def build_deb(plan: PackagePlan) -> None:
    plan.output_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["dpkg-deb", "--build", "--root-owner-group", str(plan.stage_dir), str(plan.deb_path)],
        cwd=REPO_ROOT,
        check=True,
    )


def normalize_permissions(root: Path) -> None:
    for path in root.rglob("*"):
        if path.is_dir():
            path.chmod(0o755)
        elif path.is_file():
            if path.parent == root / "usr" / "bin":
                path.chmod(0o755)
            else:
                path.chmod(0o644)
    root.chmod(0o755)


if __name__ == "__main__":
    raise SystemExit(main())
