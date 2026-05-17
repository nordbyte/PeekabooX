#!/usr/bin/env python3
from __future__ import annotations

import difflib
import subprocess
import sys
from pathlib import Path
from tempfile import TemporaryDirectory

REPO_ROOT = Path(__file__).resolve().parents[1]
PROTO = REPO_ROOT / "proto" / "peekaboox" / "v1" / "peekaboox.proto"
CHECKED_GENERATED = REPO_ROOT / "python" / "src" / "peekaboox" / "v1"


def main() -> int:
    with TemporaryDirectory() as tmpdir:
        tmp = Path(tmpdir)
        subprocess.run(
            [
                sys.executable,
                "-m",
                "grpc_tools.protoc",
                "-I",
                str(REPO_ROOT / "proto"),
                f"--python_out={tmp}",
                f"--grpc_python_out={tmp}",
                str(PROTO),
            ],
            cwd=REPO_ROOT,
            check=True,
        )
        generated = tmp / "peekaboox" / "v1"
        errors = []
        for name in ("peekaboox_pb2.py", "peekaboox_pb2_grpc.py"):
            expected = (generated / name).read_text(encoding="utf-8")
            actual = (CHECKED_GENERATED / name).read_text(encoding="utf-8")
            if expected != actual:
                diff = "\n".join(
                    difflib.unified_diff(
                        actual.splitlines(),
                        expected.splitlines(),
                        fromfile=f"checked-in/{name}",
                        tofile=f"generated/{name}",
                        lineterm="",
                    )
                )
                errors.append(f"{name} is out of sync:\n{diff}")
        if errors:
            for error in errors:
                print(f"proto-sync-check: {error}", file=sys.stderr)
            return 1
    print("proto-sync-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
