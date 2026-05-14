#!/usr/bin/env python3
from __future__ import annotations

import json
import platform
import sys


def main() -> int:
    request = json.load(sys.stdin) if not sys.stdin.isatty() else {}
    tool = request.get("tool", "system_info.uname") if isinstance(request, dict) else "system_info.uname"
    if tool != "system_info.uname":
        json.dump({"ok": False, "error": f"unknown tool: {tool}"}, sys.stdout)
        sys.stdout.write("\n")
        return 2

    info = platform.uname()
    json.dump(
        {
            "ok": True,
            "result": {
                "system": info.system,
                "node": info.node,
                "release": info.release,
                "version": info.version,
                "machine": info.machine,
                "processor": info.processor,
            },
        },
        sys.stdout,
        sort_keys=True,
    )
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
