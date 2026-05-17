# Changelog

All notable PeekabooX changes are recorded here.

Release entries use `## VERSION - YYYY-MM-DD`. Git tags use `vVERSION`.

## 1.1.2 - 2026-05-17

- 5f08978 Implement runtime hardening and modularity improvements
- ec2dfb7 Add workflow bundles and harden runtime surfaces
- b744b50 Update GitHub Actions to Node 24 majors
- 83b3584 Modularize daemon runtime
- 97a1ef0 Modularize CLI commands
- aa78c5a Add desktop example harness
- 524a92f Externalize desktop profiles
- f1078c2 Harden plugin process IO
- bc34fcc Validate Nix release version
- 6b5accf Secure MCP HTTP transports
- 47d01e1 Add end-to-end gRPC token support
- 4d30754 Fix capture and window report findings

## 1.1.1 - 2026-05-16

- bbf4975 Bump version to 1.1.1
- fe3294b Fix clawpatch report bugs
- b1defef Add desktop action diagnostics examples
- 0d03e3f Add Python desktop focus diagnostics example
- 0ac1605 Add MCP desktop focus diagnostics example
- e30f2f3 Extend MCP gRPC timeout for desktop focus
- 11979f4 Expose desktop focus diagnostics in MCP
- 63c6e7d Expose focus diagnostics over gRPC
- 1b4dda2 Add desktop focus diagnostics
- 3ebb693 Focus desktop actions before locating targets
- 8e3976c Improve desktop focus fallbacks
- 4812277 Fix launcher command clippy lint
- 685dbd4 Add Peekaboo parity CLI and MCP surfaces

## 1.1.0 - 2026-05-16

- 22ee6bc Refresh example smoke coverage list
- 082d982 Add capture DMA-BUF examples
- f6ca34f Add Plugin SDK examples
- 27e9db3 Expand hotkey command surface
- 3eced4f Expand paste command surface
- d2e9105 Prefer exact text fallback for type
- 43431c3 Expand type command surface
- e2248ea Stabilize click calculator example
- c98057a Expand click command surface
- 9234bae Expand drag command surface
- b9a4608 Fix clippy warnings in input and daemon
- d0d11fb Add desktop profile daemon parity example
- 7e9bc78 Expand desktop profile registry API
- d29c57c Expand move command targets and API surface
- 3db4930 Expand vision elements options
- d82d3d2 Expand UI state detection options
- c8cd4d6 Expand visual compare regression tooling
- 5c6330b Fix clippy response enum size
- 720e817 Add capture daemon MCP parity example
- b02d7cd Expand capture targeting and metadata
- 568eb82 Complete MCP protocol surface
- d146195 Add MCP preflight error client example
- e201be7 Expose preflight MCP error details
- 21989a0 Audit preflight decisions
- ad18198 Add agent preflight smoke example
- 52c620f Expose preflight startup flags
- 7a652e8 Add doctor preflight gating
- bdfb495 Categorize doctor diagnostics
- 3d10c13 Expose doctor diagnostics in runtime MCP
- d96d5b8 Expose capture backend diagnostics in runtime MCP
- dd0ef10 Add runtime MCP capture delta examples
- a1fb296 Add capture delta stream example
- a780c61 Enhance capture backend diagnostics
- ba72419 Add windows live smoke example
- f5c4aac Expose window filters in agent and MCP
- 837f8d2 Fix windows clippy warnings
- d5055ea Expand windows query and diagnostics
- 04960cf Fix Calculator dry-run validation
- be91538 Add semantic click dry-run to Calculator example
- fed6230 Add real Calculator elements example
- 7cc6a3a Avoid AT-SPI extents on non-components
- 3bf7d7d Expand elements lookup controls
- 91686a5 Add desktop OCR example
- f4059d0 Relax OCR smoke heading match
- 283251e Expand OCR controls and examples
- f8d8f29 Separate README badges from intro text
- a6e7676 Add README banner and MIT license
- d756343 Reorganize README and move usage docs

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
