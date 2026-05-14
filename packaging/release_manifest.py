#!/usr/bin/env python3
from __future__ import annotations

import argparse
import glob
import hashlib
import json
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_WHEEL_GLOB = REPO_ROOT / "target" / "python-wheel" / "peekaboox-*.whl"
DEFAULT_DEB_GLOB = REPO_ROOT / "target" / "dist" / "peekaboox_*.deb"
DEFAULT_DOCKER_METADATA = REPO_ROOT / "target" / "dist" / "docker-image.json"
DEFAULT_OUTPUT = REPO_ROOT / "target" / "dist" / "release-manifest.json"
DEFAULT_CHECKSUMS = REPO_ROOT / "target" / "dist" / "SHA256SUMS"


@dataclass(frozen=True, slots=True)
class VersionSet:
    cargo: str
    python_project: str
    python_package: str

    @property
    def values(self) -> dict[str, str]:
        return {
            "cargo_workspace": self.cargo,
            "python_project": self.python_project,
            "python_package": self.python_package,
        }

    @property
    def release_version(self) -> str:
        versions = set(self.values.values())
        if len(versions) != 1:
            formatted = ", ".join(f"{name}={value}" for name, value in self.values.items())
            raise ValueError(f"release versions are inconsistent: {formatted}")
        return self.cargo


@dataclass(frozen=True, slots=True)
class Artifact:
    kind: str
    path: Path
    size_bytes: int
    sha256: str

    @property
    def manifest_entry(self) -> dict[str, Any]:
        return {
            "kind": self.kind,
            "name": self.path.name,
            "path": str(self.path.relative_to(REPO_ROOT)),
            "size_bytes": self.size_bytes,
            "sha256": self.sha256,
        }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate PeekabooX release metadata and write an artifact manifest."
    )
    parser.add_argument("--expected-version", help="expected release version, optionally prefixed with v")
    parser.add_argument("--wheel", default=str(DEFAULT_WHEEL_GLOB), help="wheel artifact glob")
    parser.add_argument("--deb", default=str(DEFAULT_DEB_GLOB), help="Debian artifact glob")
    parser.add_argument(
        "--docker-metadata",
        default=str(DEFAULT_DOCKER_METADATA),
        help="optional Docker image metadata JSON artifact",
    )
    parser.add_argument("--output", default=str(DEFAULT_OUTPUT), help="manifest output path")
    parser.add_argument("--checksums", default=str(DEFAULT_CHECKSUMS), help="SHA256SUMS output path")
    parser.add_argument(
        "--check",
        action="store_true",
        help="validate versions and changelog without requiring built artifacts",
    )
    args = parser.parse_args()

    try:
        versions = read_versions()
        release_version = versions.release_version
        expected_version = normalize_expected_version(args.expected_version)
        if expected_version is not None and release_version != expected_version:
            raise ValueError(
                f"release version {release_version} does not match expected {expected_version}"
            )
        changelog_entry = validate_changelog(release_version)

        if args.check:
            print(
                json.dumps(
                    {
                        "version": release_version,
                        "versions": versions.values,
                        "changelog_entry": changelog_entry,
                    },
                    indent=2,
                    sort_keys=True,
                )
            )
            return 0

        artifacts = collect_artifacts(args.wheel, args.deb, args.docker_metadata, release_version)
        manifest = {
            "schema_version": 1,
            "generated_at": datetime.now(UTC).isoformat(timespec="seconds"),
            "version": release_version,
            "versions": versions.values,
            "git": git_info(),
            "changelog_entry": changelog_entry,
            "artifacts": [artifact.manifest_entry for artifact in artifacts],
            "docker": docker_metadata(Path(args.docker_metadata)),
        }

        output = Path(args.output)
        checksums = Path(args.checksums)
        output.parent.mkdir(parents=True, exist_ok=True)
        checksums.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        write_checksums(checksums, artifacts)
        print(json.dumps({"manifest": str(output), "checksums": str(checksums)}, sort_keys=True))
        return 0
    except Exception as exc:
        print(f"release manifest error: {exc}", file=sys.stderr)
        return 1


def read_versions() -> VersionSet:
    cargo = read_cargo_workspace_version()
    python_project = read_pyproject_version()
    python_package = read_python_package_version()
    return VersionSet(cargo=cargo, python_project=python_project, python_package=python_package)


def read_cargo_workspace_version() -> str:
    data = tomllib.loads((REPO_ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    try:
        return data["workspace"]["package"]["version"]
    except KeyError as exc:
        raise ValueError("missing workspace.package.version in Cargo.toml") from exc


def read_pyproject_version() -> str:
    data = tomllib.loads((REPO_ROOT / "python" / "pyproject.toml").read_text(encoding="utf-8"))
    try:
        return data["project"]["version"]
    except KeyError as exc:
        raise ValueError("missing project.version in python/pyproject.toml") from exc


def read_python_package_version() -> str:
    package_init = REPO_ROOT / "python" / "src" / "peekaboox" / "__init__.py"
    match = re.search(
        r'^__version__\s*=\s*["\']([^"\']+)["\']',
        package_init.read_text(encoding="utf-8"),
        re.MULTILINE,
    )
    if match is None:
        raise ValueError("missing __version__ in python/src/peekaboox/__init__.py")
    return match.group(1)


def normalize_expected_version(value: str | None) -> str | None:
    if value is None:
        return None
    version = value.strip()
    if version.startswith("refs/tags/"):
        version = version.removeprefix("refs/tags/")
    if version.startswith("v"):
        version = version[1:]
    return version


def validate_changelog(version: str) -> str:
    changelog = REPO_ROOT / "CHANGELOG.md"
    if not changelog.exists():
        raise FileNotFoundError("CHANGELOG.md is required for release validation")
    pattern = re.compile(rf"^##\s+\[?{re.escape(version)}\]?\s+-\s+(\d{{4}}-\d{{2}}-\d{{2}})\s*$")
    for line in changelog.read_text(encoding="utf-8").splitlines():
        if pattern.match(line):
            return line
    raise ValueError(f"CHANGELOG.md has no release entry for {version}")


def collect_artifacts(
    wheel_glob: str,
    deb_glob: str,
    docker_metadata_path: str,
    release_version: str,
) -> list[Artifact]:
    artifacts: list[Artifact] = []
    wheel = resolve_one(wheel_glob, "wheel")
    deb = resolve_one(deb_glob, "Debian package")
    validate_artifact_version("wheel", wheel, f"peekaboox-{release_version}-")
    validate_artifact_version("Debian package", deb, f"peekaboox_{release_version}_")
    artifacts.append(describe_artifact("python_wheel", wheel))
    artifacts.append(describe_artifact("debian_package", deb))

    docker_metadata_file = Path(docker_metadata_path)
    if docker_metadata_file.exists():
        artifacts.append(describe_artifact("docker_metadata", docker_metadata_file))
    return artifacts


def resolve_one(pattern: str, label: str) -> Path:
    matches = sorted(Path(path) for path in glob.glob(pattern))
    if not matches:
        raise FileNotFoundError(f"no {label} artifact matched {pattern}")
    return matches[-1]


def validate_artifact_version(label: str, path: Path, expected_prefix: str) -> None:
    if not path.name.startswith(expected_prefix):
        raise ValueError(
            f"{label} artifact {path.name} does not match release version prefix {expected_prefix}"
        )


def describe_artifact(kind: str, path: Path) -> Artifact:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return Artifact(
        kind=kind,
        path=path.resolve(),
        size_bytes=path.stat().st_size,
        sha256=digest.hexdigest(),
    )


def write_checksums(path: Path, artifacts: list[Artifact]) -> None:
    lines = [
        f"{artifact.sha256}  {artifact.path.relative_to(REPO_ROOT)}"
        for artifact in sorted(artifacts, key=lambda item: item.path.name)
    ]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def git_info() -> dict[str, str | None]:
    return {
        "commit": run_git("rev-parse", "HEAD"),
        "ref": run_git("rev-parse", "--abbrev-ref", "HEAD"),
        "tag": run_git("describe", "--tags", "--exact-match"),
    }


def run_git(*args: str) -> str | None:
    try:
        return subprocess.check_output(
            ["git", *args],
            cwd=REPO_ROOT,
            stderr=subprocess.DEVNULL,
            text=True,
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def docker_metadata(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {"path": str(path.relative_to(REPO_ROOT)), "parse_error": "invalid json"}
    if isinstance(data, list) and data:
        data = data[0]
    if isinstance(data, dict):
        return data
    return {"path": str(path.relative_to(REPO_ROOT)), "metadata": data}


if __name__ == "__main__":
    raise SystemExit(main())
