# Vision Fixtures

Small text-encoded image fixtures for vision regression tests.

- `baseline.ppm` and `changed.ppm` cover low-level visual diff and UI-state
  transitions.
- `ui_controls.pbm` models a simple screen with two visible controls for
  decoder-backed UI-element detection and vision-fallback tests.
- `ui_controls_loading.pbm` adds a transient progress region to the same screen
  for loading-state regression coverage.

The fixtures intentionally use plain PNM formats (`P3` PPM and `P1` PBM) so
they remain reviewable in source control while still exercising the image
decoder path used by file-based APIs.
