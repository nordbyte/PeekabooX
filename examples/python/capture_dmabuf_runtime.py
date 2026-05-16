#!/usr/bin/env python3
from __future__ import annotations

import json

from peekaboox.agent import AgentRuntime
from peekaboox.client import DmaBufProbeResult


class RecordingClient:
    def __init__(self) -> None:
        self.calls: list[str] = []

    def probe_dmabuf(self, import_target: str = "compute") -> DmaBufProbeResult:
        normalized = import_target.strip().casefold().replace("-", "_")
        if normalized not in {"compute", "egl", "egl_texture"}:
            raise ValueError("import_target must be compute, egl, or egl_texture")
        self.calls.append(normalized)
        return DmaBufProbeResult(
            import_target=normalized,
            backend_name="fake-dmabuf",
            stream_node_id=7,
            pipewire_serial=11,
            width=800,
            height=600,
            pixel_format="rgba8",
            fourcc=875_713_112,
            planes=1,
            memory_layout="single-plane",
            synchronization="implicit",
            egl_version="1.5" if normalized != "compute" else None,
            egl_modifiers=True if normalized != "compute" else None,
            texture_id=42 if normalized == "egl_texture" else None,
        )


def main() -> int:
    client = RecordingClient()
    runtime = AgentRuntime(client=client)

    compute = runtime.probe_dmabuf("compute")
    texture = runtime.probe_dmabuf("egl-texture")

    assert compute.import_target == "compute"
    assert compute.backend_name == "fake-dmabuf"
    assert compute.planes == 1
    assert texture.import_target == "egl_texture"
    assert texture.texture_id == 42

    summary = {
        "calls": client.calls,
        "compute": {
            "backend_name": compute.backend_name,
            "width": compute.width,
            "height": compute.height,
            "pixel_format": compute.pixel_format,
        },
        "egl_texture": {
            "egl_version": texture.egl_version,
            "texture_id": texture.texture_id,
        },
    }
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
