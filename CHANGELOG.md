# Changelog

All notable PeekabooX changes are recorded here.

Release entries use `## VERSION - YYYY-MM-DD`. Git tags use `vVERSION`.

## 1.0.1 - 2026-05-15

- e6f3742 Add desktop scoping, verification, and diagnostics
- 6130ec1 Expose desktop helper APIs and capture targets
- 17581b1 Close API parity and plugin hardening gaps
- 1e6f89d Fix clipboard paste command hang
- 7c81414 Add clipboard paste input command
- 930434a Fix Text Editor layout clippy warnings
- ad3aeae Add safe Text Editor desktop example
- 912e472 Tighten paint canvas detection
- c2a10dd Add paint desktop automation targets
- 9731d88 Add reusable desktop automation helper
- 38a88bd Make Telegram desktop example dynamic
- bbef234 Harden Telegram desktop example
- b3d5833 Add Telegram Saved Messages desktop example
- 2963b1a Improve Wayland pointer input live example
- b281536 Fix input drag clippy warning
- 1f1e1e9 Add pointer input actions and desktop examples
- d92186b Add runnable examples and smoke checks

## 1.0.0 - 2026-05-14

- Implemented the Linux desktop automation foundation:
  daemon/CLI control, accessibility-first semantic lookup, vision/OCR fallbacks,
  MCP runtime, workflow recorder/replay, security gates, performance paths,
  plugin SDK foundation, and local packaging.
- Added installable artifacts for Rust binaries, Python wheels, Debian packages,
  Docker smoke images, and Nix development/build entry points.
