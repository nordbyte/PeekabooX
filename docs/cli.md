# CLI and Desktop Usage

This page keeps the operational CLI details that used to live in the root
README. The short command overview stays in [README.md](../README.md).

## Current Entry Points

Useful local validation commands:

```bash
cargo check --workspace
python3 -m compileall python/src
PYTHONPATH=python/src python3 benchmarks/perf_baseline.py --iterations 30
cargo run -q -p peekaboox-cli -- doctor --json
```

Build local install artifacts:

```bash
packaging/install-rust.sh
python3 -m pip wheel --no-deps -w target/python-wheel ./python
python3 packaging/debian/build_deb.py --check
python3 packaging/debian/build_deb.py
python3 packaging/smoke_install.py --skip-cargo-install
python3 packaging/release_manifest.py
docker build --target smoke -t peekaboox:smoke .
```

## Capture

Create a screenshot in the current Linux desktop session:

```bash
cargo run -q -p peekaboox-cli -- capture --output screenshot.png
cargo run -q -p peekaboox-cli -- capture --region 10,20,400,240 --output region.png
cargo run -q -p peekaboox-cli -- --daemon capture --window-id window-1 --output window.png
```

The capture implementation detects the session and prefers
`xdg-desktop-portal` on Wayland, with GNOME, wlroots, KDE, and X11 command
fallbacks where available. Incremental capture first tries direct
stdout-to-frame capture through backends that can emit image bytes without a
daemon-managed screenshot file, then falls back to file-only capture backends.

## Capture Backends and DMA-BUF

Use `capture-backends` to inspect image backends and whether the optional
Portal/PipeWire DMA-BUF zero-copy path is available on the current session.
Feature builds require native `libpipewire-0.3`, `libspa-0.2`, `libEGL`, and
`libGLESv2` development packages.

```bash
cargo run -q -p peekaboox-cli -- capture-backends
cargo run -q -p peekaboox-cli --features pipewire-backend -- capture-dmabuf
cargo run -q -p peekaboox-cli --features pipewire-backend,egl-backend -- capture-dmabuf --import egl
cargo run -q -p peekaboox-cli --features pipewire-backend,egl-backend -- capture-dmabuf --import egl-texture
cargo run -q -p peekaboox-cli --features pipewire-backend,egl-backend -- --daemon capture-dmabuf --import egl-texture
```

The Rust capture API can open a Portal ScreenCast session and PipeWire remote
through `open_pipewire_screencast()`. Builds compiled with the
`pipewire-backend` feature add a PipeWire stream consumer that negotiates
DMA-BUF buffers and returns frame/plane descriptors. The same API exposes a
validated DMA-BUF import descriptor for EGL, Vulkan, or compute backends, and
the optional `egl-backend` feature can import that descriptor into a native
`EGLImage` or bind it as a GLES `GL_TEXTURE_2D` without copying through CPU
image bytes.

## Capture Delta

`capture-delta` keeps previous-frame state per stream and returns a full-frame
patch first, then only changed rectangles when possible:

```bash
peekaboox --daemon capture-delta --stream agent-loop --threshold 2
peekaboox --daemon capture-delta --stream agent-loop --window-id window-1
peekaboox --daemon capture-delta --stream agent-loop --low-bandwidth
peekaboox --daemon capture-delta --stream agent-loop --full-frame
peekaboox --daemon capture-delta --stream agent-loop --reset
```

## Input Actions

Check or execute basic input automation:

```bash
cargo run -q -p peekaboox-cli -- click --x 100 --y 200 --dry-run
cargo run -q -p peekaboox-cli -- click --text "Submit" --dry-run
cargo run -q -p peekaboox-cli -- move --x 100 --y 200 --dry-run
cargo run -q -p peekaboox-cli -- drag --from 100,200 --to 320,240 --dry-run
cargo run -q -p peekaboox-cli -- type --dry-run "Hello World"
cargo run -q -p peekaboox-cli -- paste --dry-run "/tmp/PeekabooX Example.txt"
cargo run -q -p peekaboox-cli -- type --paste --preserve-clipboard --dry-run "/tmp/PeekabooX Example.txt"
cargo run -q -p peekaboox-cli -- hotkey --dry-run ctrl+s
```

Remove `--dry-run` to perform the action. Input prefers direct `/dev/uinput`
pointer events on Wayland, uses `ydotool` for Wayland hotkeys, prefers `wtype`
for Wayland text where available with `ydotool` as fallback, and prefers
`xdotool` on X11. Clipboard paste uses `wl-copy`, `xclip`, or `xsel` plus the
safest available `ctrl+v` backend, can restore the previous textual clipboard
with `--preserve-clipboard`, and is better than synthetic typing for paths and
layout-sensitive text.

Semantic click targets use AT-SPI and resolve to the center of the matching UI
element. Add `--vision-fallback` when a semantic lookup may need screenshot
analysis:

```bash
peekaboox click --selector "role=button" --vision-fallback --dry-run
peekaboox --daemon elements --selector "role=button" --vision-fallback
```

## Desktop Helpers

Use the higher-level desktop helper when an action needs app focus, screenshot
layout detection, and guards around state-sensitive targets:

```bash
cargo run -q -p peekaboox-cli -- desktop profiles
cargo run -q -p peekaboox-cli -- desktop profiles --json
cargo run -q -p peekaboox-cli -- desktop focus --app telegram
cargo run -q -p peekaboox-cli -- desktop locate --app telegram --target search-input
cargo run -q -p peekaboox-cli -- desktop type-into --app telegram --target search-input --clear "Saved Messages"
cargo run -q -p peekaboox-cli -- desktop assert --app telegram --target send-button --not-active
cargo run -q -p peekaboox-cli -- desktop drag --app drawing --target canvas --from-ratio 0.2,0.3 --to-ratio 0.8,0.3
cargo run -q -p peekaboox-cli -- desktop type-into --app text-editor --target document --window-title notes.txt --clear "PeekabooX"
cargo run -q -p peekaboox-cli -- --daemon desktop click --app telegram --target search-input --dry-run --verify --json
```

Built-in profiles include `telegram`, `paint`, `drawing`, `pinta`,
`kolourpaint`, and `text-editor`. They use the safest available path in order:
existing window focus where window enumeration is available, GNOME Overview or
application launch fallback, then app-specific visual layout targets such as
Telegram's `search-input` and `message-input`, Paint's `canvas`, or Text
Editor's `document`.

Pass `--window-id <id>` or `--window-title <text>` to `desktop focus`,
`locate`, `click`, `drag`, `type-into`, or `assert` when an action must target
one specific window instead of the currently focused matching app. Mutating
desktop helper actions accept `--verify` to run a postcondition check after the
action. `desktop profiles --json` exposes the built-in app/target registry for
scripts and plugin authors.

The same desktop-helper surface is exposed through daemon JSON IPC, gRPC,
`PeekabooXClient`, `AgentRuntime`, and MCP tools: `desktop_focus`,
`desktop_locate`, `desktop_click`, `desktop_drag`, `desktop_type_into`, and
`desktop_assert`.

## Windows and Elements

List visible desktop windows:

```bash
cargo run -q -p peekaboox-cli -- windows
cargo run -q -p peekaboox-cli -- windows --json
cargo run -q -p peekaboox-cli -- windows --focused --limit 1 --sort focused --json
cargo run -q -p peekaboox-cli -- windows --app calculator --title-regex "Calculator" --diagnose --json
cargo run -q -p peekaboox-cli -- windows --backend xdotool --diagnose
```

Window enumeration tries GNOME Shell Introspect on GNOME, falls back to AT-SPI
for Wayland-accessible applications, and uses `xdotool` for X11/XWayland
windows. `windows` supports `--id <id>`, `--app <app>`, `--title <text>`,
`--title-regex <regex>`, `--focused`, `--limit <n>`, `--sort
backend|focused|title|app|area|id|state`, `--backend
auto|gnome|at-spi|xdotool`, and `--diagnose`. JSON responses include
`backend_name`, `backend_kind`, `warnings`, and per-backend diagnostic reports
so scripts can see which backend was selected and why fallbacks were attempted.

List semantic elements from the CLI:

```bash
peekaboox elements --role "push button" --state enabled --contains 100,200
peekaboox elements --selector "role=push button" --vision-fallback
peekaboox --daemon find --selector "role=push button,label=Submit,confidence>=0.9"
peekaboox elements --window-title "Draft" --role-exact "push button" --text-regex "^Save"
peekaboox elements --app text-editor --selector "not-state=disabled,min-width=40" --json
```

`elements` accepts `--app`, `--window-title`, and `--window-id` to scope
semantic AT-SPI matches and screenshot fallback to the intended window. Selector
parsing is strict: malformed rectangles, points, numbers, regexes, or unknown
keys return an error instead of broadening the match. Supported selector keys
include `id`, `role`, `label`/`text`, `state`, `not-state`, exact and regex
variants such as `role-exact` and `label-regex`, `bounds`, `contains`,
`within`, `intersects`, `min-width`, `min-height`, and `confidence>=`.

When `--vision-fallback` is enabled, the detector can be tuned with
`--vision-region`, `--vision-threshold`, `--vision-min-width`,
`--vision-min-height`, `--vision-min-component-pixels`,
`--vision-max-elements`, and `--vision-merge-distance`. JSON output includes
each element's `center`, window/app hierarchy metadata when available, and
daemon lookup metadata such as cache and fallback status.

## Vision Tools

OCR expects the `tesseract` executable on `PATH`. Set
`PEEKABOOX_OCR_LANGUAGE=eng` or another installed language code to override the
default language. OCR can run against the live screen, a screen region, a
window target, or an existing image file. Region OCR crops before invoking
Tesseract and maps returned bounds back to the source coordinate space:

```bash
peekaboox ocr --region 10,20,400,120 --language eng
peekaboox ocr --image tests/fixtures/ocr/ocr_sample.png --psm 6 --json
peekaboox ocr --window-id window-1 --words
peekaboox --daemon ocr
```

Tesseract tuning and preprocessing are exposed through `--psm <0-13>`,
`--oem <0-3>`, `--dpi <n>`, `--min-confidence <0..1>`,
`--whitelist <chars>`, repeated `--config key=value`, `--scale`,
`--grayscale`, `--threshold <0-255>`, `--invert`, `--contrast`, and
`--deskew`.

Visual comparison primitives support region diffing and action verification:

```bash
peekaboox compare before.png after.png --max-changed-ratio 0.01
peekaboox --daemon compare --expected before.png --actual after.png --threshold 3
```

UI-state detection classifies a sampled screen sequence as stable, loading, or
changing:

```bash
peekaboox state frame1.png frame2.png frame3.png
peekaboox --daemon state --image frame1.png --image frame2.png
```

Vision-only UI element detection groups contrast and edge components into
fallback regions:

```bash
peekaboox vision-elements screenshot.png --min-width 8
peekaboox --daemon vision-elements --image screenshot.png --max-elements 25
```

Regression image fixtures live in `tests/fixtures/vision`.

## Doctor

Check the current environment before running live automation:

```bash
cargo run -q -p peekaboox-cli -- doctor
cargo run -q -p peekaboox-cli -- doctor --json
cargo run -q -p peekaboox-cli -- doctor --strict
```

`doctor` reports display/session state, helper commands, capture, DMA-BUF,
window enumeration, input, OCR, Python gRPC imports, and desktop profiles.

## Daemon Routing

Run the local daemon API and send CLI commands through it:

```bash
cargo run -q -p peekabooxd -- run
cargo run -q -p peekaboox-cli -- --daemon windows
```

The daemon listens on `127.0.0.1:47777` for gRPC by default using
`proto/peekaboox/v1/peekaboox.proto`. It also listens on
`$XDG_RUNTIME_DIR/peekabooxd.sock` for the CLI's newline-delimited JSON
protocol. Daemon-side semantic queries use a short AT-SPI cache with a 500ms
default TTL; override it with:

```bash
peekabooxd run --accessibility-cache-ttl-ms 250
```

Start the daemon with `--vision-fallback` or set `PEEKABOOX_VISION_FALLBACK=1`
to let semantic element and click requests fall back to a live screenshot-based
detector when accessibility returns no match. Real daemon-routed input requires
`peekabooxd run --profile operator`, `--allow-input`, or
`PEEKABOOX_ALLOW_INPUT=1`; see [docs/security.md](security.md).

## Plugins

Plugins use the declarative SDK manifest `peekaboox.plugin.json`:

```bash
peekaboox plugins --path examples/plugins
peekaboox plugins --path examples/plugins --json
peekaboox plugin-call org.peekaboox.examples.system-info system_info.uname --path examples/plugins --json
```

See [docs/plugins.md](plugins.md) and `examples/plugins/system-info`.
