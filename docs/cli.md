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
cargo run -q -p peekaboox-cli -- capture --app calculator --title-regex Calculator --json --output calculator.png
cargo run -q -p peekaboox-cli -- capture --window-id window-1 --region 10,10,220,160 --output window-region.png
cargo run -q -p peekaboox-cli -- capture --stdout > screenshot.png
cargo run -q -p peekaboox-cli -- capture --format jpeg --quality 85 --output screenshot.jpg
cargo run -q -p peekaboox-cli -- capture --format xwd --output screenshot.xwd
cargo run -q -p peekaboox-cli -- see --annotate --json
```

The capture implementation detects the session and prefers
`xdg-desktop-portal` on Wayland, with GNOME, wlroots, KDE, and X11 command
fallbacks where available. Incremental capture first tries direct
stdout-to-frame capture through backends that can emit image bytes without a
daemon-managed screenshot file, then falls back to file-only capture backends.
`capture` can target a full screen, absolute region, exact `--window-id`, or
window filters via `--app`, `--window-title`, and `--title-regex`. A `--region`
combined with a window filter is interpreted relative to that window. Add
`--format png|jpeg|xwd` controls file output; JPEG accepts `--quality 1..100`.
`--json` adds structured metadata (`width`, `height`, `mime_type`,
`capture_region`, `window_id`, `source`, and timestamp), `--include-semantic-tree`
to embed current accessibility elements in that JSON response, `--stdout` to
emit PNG bytes, and `--no-overwrite` to reject existing output files.
`see`/`observe` persists a snapshot directory under
`$XDG_STATE_HOME/peekaboox/snapshots` with the captured image, semantic capture
metadata, and an optional vision overlay from `--annotate`.

## Capture Backends and DMA-BUF

Use `capture-backends` to inspect image backends and whether the optional
Portal/PipeWire DMA-BUF zero-copy path is available on the current session. Add
`--diagnose` to include missing or skipped backends with reasons, `--json` for
machine-readable output, `--output` or `--format png|xwd` to evaluate a specific
file target, and `--probe file|frame|region|dmabuf|all` to run live backend
checks. Probe failures are reported in the output and make the command exit
non-zero. Feature builds require native `libpipewire-0.3`, `libspa-0.2`,
`libEGL`, and `libGLESv2` development packages.

```bash
cargo run -q -p peekaboox-cli -- capture-backends
cargo run -q -p peekaboox-cli -- capture-backends --diagnose --json
cargo run -q -p peekaboox-cli -- capture-backends --format xwd --diagnose
cargo run -q -p peekaboox-cli -- capture-backends --probe frame --json
cargo run -q -p peekaboox-cli -- --daemon capture-backends --diagnose --json
cargo run -q -p peekaboox-cli --features pipewire-backend -- capture-dmabuf
cargo run -q -p peekaboox-cli --features pipewire-backend,egl-backend -- capture-dmabuf --import egl
cargo run -q -p peekaboox-cli --features pipewire-backend,egl-backend -- capture-dmabuf --import egl-texture
cargo run -q -p peekaboox-cli --features pipewire-backend,egl-backend -- --daemon capture-dmabuf --import egl-texture
examples/cli/capture_dmabuf_probe.sh
```

The Rust capture API can open a Portal ScreenCast session and PipeWire remote
through `open_pipewire_screencast()`. Builds compiled with the
`pipewire-backend` feature add a PipeWire stream consumer that negotiates
DMA-BUF buffers and returns frame/plane descriptors. The same API exposes a
validated DMA-BUF import descriptor for EGL, Vulkan, or compute backends, and
the optional `egl-backend` feature can import that descriptor into a native
`EGLImage` or bind it as a GLES `GL_TEXTURE_2D` without copying through CPU
image bytes.
`examples/cli/capture_dmabuf_probe.sh` always validates the command and
diagnostic zero-copy surface, and only runs the live probe when
`PEEKABOOX_DMABUF_LIVE=1` is set.

## Capture Delta

`capture-delta` keeps previous-frame state per daemon stream and returns a
full-frame patch first, then only changed rectangles when possible. Use `--json`
when scripts need the sequence, `full_frame`, `capture_region`,
`changed_bounds`, patch metadata, and backend fields as structured data:

```bash
peekaboox --daemon capture-delta --stream agent-loop --threshold 2
peekaboox --daemon capture-delta --stream agent-loop --window-id window-1
peekaboox --daemon capture-delta --stream agent-loop --low-bandwidth
peekaboox --daemon capture-delta --stream agent-loop --full-frame
peekaboox --daemon capture-delta --stream agent-loop --reset
peekaboox --daemon capture-delta --stream agent-loop --json
```

## Input Actions

Check or execute basic input automation:

```bash
cargo run -q -p peekaboox-cli -- click --x 100 --y 200 --dry-run
cargo run -q -p peekaboox-cli -- click --to 100,200 --backend xdotool --bounds clamp --dry-run --json
cargo run -q -p peekaboox-cli -- click --window-title Calculator --ratio 0.5,0.5 --restore --dry-run --json
cargo run -q -p peekaboox-cli -- click --text "Submit" --dry-run
cargo run -q -p peekaboox-cli -- move --x 100 --y 200 --dry-run
cargo run -q -p peekaboox-cli -- move --to 100,200 --duration-ms 180 --steps 8 --dry-run --json
cargo run -q -p peekaboox-cli -- move --relative 20,0 --backend xdotool --dry-run
cargo run -q -p peekaboox-cli -- move --region 0,0,400,240 --ratio 0.5,0.5 --clamp --dry-run
cargo run -q -p peekaboox-cli -- move --window-title Calculator --ratio 0.5,0.5 --dry-run
cargo run -q -p peekaboox-cli -- move --current-position --json
cargo run -q -p peekaboox-cli -- drag --from 100,200 --to 320,240 --dry-run
cargo run -q -p peekaboox-cli -- drag --from-current --region 0,0,400,240 --to-ratio 0.8,0.5 --steps 10 --backend xdotool --bounds clamp --restore --dry-run --json
cargo run -q -p peekaboox-cli -- swipe --from 100,400 --to 100,120 --dry-run
cargo run -q -p peekaboox-cli -- type --dry-run "Hello World"
cargo run -q -p peekaboox-cli -- type --backend wtype --typing-speed 20 --delay-ms 100 --dry-run --json --text "Hello World"
cargo run -q -p peekaboox-cli -- type --file ./message.txt --key-delay-ms 15 --dry-run
printf 'multi-line\ntext\n' | cargo run -q -p peekaboox-cli -- type --stdin --dry-run
cargo run -q -p peekaboox-cli -- paste --dry-run "/tmp/PeekabooX Example.txt"
cargo run -q -p peekaboox-cli -- paste --clipboard-backend wl-copy --hotkey-backend ydotool --delay-ms 80 --restore-delay-ms 120 --restore-policy best-effort --preserve-clipboard --dry-run --json --text "/tmp/PeekabooX Example.txt"
cargo run -q -p peekaboox-cli -- type --paste --preserve-clipboard --dry-run "/tmp/PeekabooX Example.txt"
cargo run -q -p peekaboox-cli -- hotkey --backend auto --delay-ms 25 --key-delay-ms 30 --repeat 2 --interval-ms 40 --release-before --release-after --dry-run --json control+s
cargo run -q -p peekaboox-cli -- hotkey --dry-run -- --help
cargo run -q -p peekaboox-cli -- press enter --dry-run --json
cargo run -q -p peekaboox-cli -- scroll down --amount 5 --at 400,300 --dry-run --json
```

Remove `--dry-run` to perform the action. `click` accepts absolute coordinates,
compact `--to x,y`, semantic selectors, region/window ratio targets, structured
`--json` output, bounds policies with `--clamp` or `--fail-out-of-bounds`,
optional cursor `--restore`, button selection, and backend selection via
`--backend auto|uinput|ydotool|xdotool`. `move` accepts absolute coordinates,
compact `--to x,y`, relative deltas, region/window ratio targets, smooth
movement with `--duration-ms` and `--steps`, bounds policies with `--clamp` or
`--fail-out-of-bounds`, cursor query via `--current-position`, optional
`--restore`, and backend selection via `--backend auto|uinput|ydotool|xdotool`.
Input prefers direct `/dev/uinput` pointer events on Wayland, uses `ydotool` for
Wayland hotkeys, prefers `wtype` for Wayland text where available with
clipboard paste as the exact-text fallback before layout-sensitive `ydotool`,
and prefers `xdotool` on X11. Text input accepts
`--backend auto|wtype|ydotool|xdotool`, `--typing-speed <chars-per-second>`,
explicit `--key-delay-ms`, optional initial `--delay-ms`, structured `--json`
output, and text from positional arguments, `--text`, `--stdin`, or `--file`.
Use `--` before literal text that starts with a dash. Clipboard paste uses
`wl-copy`, `xclip`, or `xsel` plus the safest available `ctrl+v` backend, can
restore the previous textual clipboard with `--preserve-clipboard`, and is
better than synthetic typing for paths and layout-sensitive text. Paste accepts
`--clipboard-backend auto|wl-copy|xclip|xsel`, `--hotkey-backend
auto|ydotool|xdotool`, `--delay-ms`, `--restore-delay-ms`, `--restore-policy
strict|best-effort|off`, `--dry-run`, `--json`, and the same `--text`,
`--stdin`, `--file`, positional, and `--` text sources as `type`. Hotkeys
accept positional chords such as `ctrl+s`, split `+`-separated aliases such as
`control`, `escape`, and `win`, and support `--backend auto|ydotool|xdotool`,
`--delay-ms`, `--key-delay-ms`, `--repeat`, `--interval-ms`,
`--release-before`, `--release-after`, `--json`, and `--dry-run`. `press` is a
key-press convenience wrapper around the same hotkey backend with modifier
release guards. `scroll` sends wheel events through `ydotool` or `xdotool` and
accepts `up|down|left|right`, `--amount`, optional `--at x,y`, bounds policy,
and dry-run JSON output. `swipe` is a drag preset with a touch-like duration.
Use `--` before literal key names that start with a dash.

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
cargo run -q -p peekaboox-cli -- desktop profiles --app telegram --supports type-into --target message-input --json
cargo run -q -p peekaboox-cli -- desktop profiles --availability --installed --json
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
existing window focus through the window manager or AT-SPI, GNOME Overview
activation, coordinate click only as the last focus fallback, application launch
when no matching window is available, then app-specific visual layout targets
such as Telegram's `search-input` and `message-input`, Paint's `canvas`, or
Text Editor's `document`.

Pass `--window-id <id>` or `--window-title <text>` to `desktop focus`,
`locate`, `click`, `drag`, `type-into`, or `assert` when an action must target
one specific window instead of the currently focused matching app. Mutating
desktop helper actions (`click`, `drag`, and `type-into`) first focus the
target app/window through the same safe focus path before resolving live
coordinates. `--dry-run` and `--image` keep this focus step disabled so previews
and offline screenshot analysis remain side-effect free. Mutating desktop
helper actions accept `--verify` to run a postcondition check after the action.
Focus actions and live actions that pre-focus an app include compact
`focus_diagnostics` in JSON output and print the same diagnostics on stderr in
text mode. These entries show the window lookup, attempted focus strategies,
skipped fallbacks, and verification outcome so moved or hidden windows can be
debugged without guessing which path ran.
`desktop profiles --json` exposes the built-in app/target registry for scripts
and plugin authors, including `schema_version`, `count`, full launch commands
with arguments, per-target capability metadata, and optional availability
checks. Filter it with `--app`, `--target`, `--command`, `--desktop-id`,
`--supports`, `--installed`, or `--available`; use `--availability`/`--check` to
annotate commands and desktop ids without filtering.

Additional JSON profiles are loaded from `PEEKABOOX_DESKTOP_PROFILE_PATH`
using colon-separated files or directories, then from
`$XDG_CONFIG_HOME/peekaboox/desktop-profiles`,
`~/.config/peekaboox/desktop-profiles`, `/etc/peekaboox/desktop-profiles`, and
`/usr/share/peekaboox/desktop-profiles`. A profile file uses
`"schema_version": "desktop-profile.v1"` and may contain either one profile or
a top-level `profiles` array. External profiles can reuse built-in target
layouts with `kind: "telegram"`, `kind: "paint"`, or `kind: "text-editor"`, or
use `kind: "generic"` for window-relative targets:

```json
{
  "schema_version": "desktop-profile.v1",
  "id": "calculator",
  "kind": "generic",
  "aliases": ["calc", "gnome-calculator"],
  "search_name": "Calculator",
  "desktop_ids": ["org.gnome.Calculator"],
  "commands": [{"program": "gnome-calculator"}],
  "targets": [
    {
      "name": "display",
      "supports": ["assert-contains"],
      "visual": {
        "type": "relative-rect",
        "x": 0.08,
        "y": 0.08,
        "width": 0.84,
        "height": 0.24
      }
    }
  ]
}
```

Generic profiles automatically get a `window` target if none is declared.
External profiles with the same `id` as a built-in profile override the built-in
entry, which allows downstream packages to tune launch commands or aliases
without recompiling PeekabooX.

The same desktop-helper surface is exposed through daemon JSON IPC, gRPC,
`PeekabooXClient`, `AgentRuntime`, and MCP tools: `desktop_focus`,
`desktop_profiles`, `desktop_locate`, `desktop_click`, `desktop_drag`,
`desktop_type_into`, and `desktop_assert`.

## Windows and Elements

List visible desktop windows:

```bash
cargo run -q -p peekaboox-cli -- windows
cargo run -q -p peekaboox-cli -- windows --json
cargo run -q -p peekaboox-cli -- windows --focused --limit 1 --sort focused --json
cargo run -q -p peekaboox-cli -- windows --app calculator --title-regex "Calculator" --diagnose --json
cargo run -q -p peekaboox-cli -- windows --backend xdotool --diagnose
cargo run -q -p peekaboox-cli -- window focus --app calculator
cargo run -q -p peekaboox-cli -- window move --id 12345 --x 20 --y 40 --dry-run
cargo run -q -p peekaboox-cli -- window resize --id 12345 --width 900 --height 640 --dry-run
```

Window enumeration tries GNOME Shell Introspect on GNOME, falls back to AT-SPI
for Wayland-accessible applications, and uses `xdotool` for X11/XWayland
windows. `windows` supports `--id <id>`, `--app <app>`, `--title <text>`,
`--title-regex <regex>`, `--focused`, `--limit <n>`, `--sort
backend|focused|title|app|area|id|state`, `--backend
auto|gnome|at-spi|xdotool`, and `--diagnose`. JSON responses include
`backend_name`, `backend_kind`, `warnings`, and per-backend diagnostic reports
so scripts can see which backend was selected and why fallbacks were attempted.
The singular `window` command adds action-oriented helpers for resolved window
IDs or filters: `list`, `focus`, `close`, `minimize`, `move`, `resize`,
`maximize`, `unmaximize`, and `fullscreen`. X11 actions use `xdotool` and
workspace/state actions use `wmctrl` when available.

List semantic elements from the CLI:

```bash
peekaboox elements --role "push button" --state enabled --contains 100,200
peekaboox elements --selector "role=push button" --vision-fallback
peekaboox --daemon find --selector "role=push button,label=Submit,confidence>=0.9"
peekaboox elements --window-title "Draft" --role-exact "push button" --text-regex "^Save"
peekaboox elements --app text-editor --selector "not-state=disabled,min-width=40" --json
peekaboox set-value --selector "role=text,label=Name" --value "Ada" --dry-run
peekaboox perform-action --selector "role=push button,label=Save" --action click --dry-run
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

`set-value` and `perform-action` use direct AT-SPI interfaces when the matched
element exposes `EditableText`, `Value`, or `Action`. They are useful for stable
semantic automation when coordinate clicks or synthetic text input are less
reliable. Use `--dry-run` first to confirm the matched element.

## Snapshots, Agent, and System Commands

The CLI includes utility surfaces for the remaining automation loop:

```bash
peekaboox see --id before-save --annotate --json
peekaboox agent --goal "Inspect Calculator and report visible buttons" --dry-run --json
peekaboox agent list-sessions --json
peekaboox app list --json
peekaboox app launch org.gnome.Calculator.desktop
peekaboox launcher open org.gnome.TextEditor.desktop
peekaboox workspace list
peekaboox workspace switch 1
peekaboox dialog list --json
peekaboox dialog accept
peekaboox menu list --json
peekaboox menu click File
peekaboox config init
peekaboox config set input.backend '"xdotool"'
peekaboox permissions --json
peekaboox tools
peekaboox completions bash
peekaboox clean --dry-run --json
```

`agent` is a local deterministic session wrapper with persisted JSON session
state under `$XDG_STATE_HOME/peekaboox/agent/sessions`. Non-dry-run sessions
capture an initial `see` snapshot; model-backed execution can build on the same
session file later. `app`, `launcher`, `workspace`, `dialog`, and `menu` provide
Linux equivalents for application launch/focus, launcher entries, virtual
desktops, dialogs, menu bars, and status/menu items using desktop files,
AT-SPI, `xdotool`, and `wmctrl` where available. `config`, `permissions`,
`tools`, `completions`, and `clean` expose local configuration, capability
checks, machine-readable command metadata, shell completion snippets, and
snapshot/session cache cleanup.

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

Visual comparison primitives support region diffing, visual-regression gates,
ignore masks, size policies, and action verification:

```bash
peekaboox compare before.png after.png --max-changed-ratio 0.01
peekaboox compare before.png after.png --ignore-region 10,20,80,24 --max-changed-pixels 100 --diff-output diff.png --report diff.json
peekaboox --daemon compare --expected before.png --actual after.png --threshold 3 --size-policy common-region
```

`compare` accepts `--region`, repeated `--ignore-region`, `--threshold`,
`--max-changed-ratio`, `--max-changed-pixels`, `--max-mae`,
`--max-channel-delta`, `--size-policy error|common-region|resize-actual`,
`--alpha ignore|compare`, `--diff-output <path>`, `--report <path>`,
`--no-fail`, and `--json`. The default size policy rejects mismatched image
dimensions; `common-region` compares the shared top-left area, and
`resize-actual` scales the actual image to the expected image dimensions before
comparison.

UI-state detection classifies a sampled screen sequence as stable, loading, or
changing:

```bash
peekaboox state frame1.png frame2.png frame3.png
peekaboox state frame1.png frame2.png --ignore-region 10,20,80,24 --stable-max-changed-pixels 20 --loading-min-changed-pixels 200
peekaboox --daemon state --image frame1.png --image frame2.png --size-policy common-region
```

`state` accepts `--region`, repeated `--ignore-region`, `--threshold`,
`--stable-max-changed-ratio`, `--stable-max-changed-pixels`,
`--stable-max-mae`, `--stable-max-channel-delta`,
`--loading-min-changed-ratio`, `--loading-min-changed-pixels`,
`--required-stable-transitions`, `--size-policy
error|common-region|resize-actual`, `--alpha ignore|compare`, and `--json`.
Stable classification uses all stable gates; loading classification can use
either the loading ratio or the loading changed-pixel threshold.

Vision-only UI element detection groups contrast and edge components into
fallback regions:

```bash
peekaboox vision-elements screenshot.png --min-width 8
peekaboox vision-elements --image screenshot.png --region 10,20,400,300 --ignore-region 10,20,80,24 --min-confidence 0.8 --sort confidence --mask-output mask.png --overlay-output overlay.png --json
peekaboox --daemon vision-elements --image screenshot.png --max-elements 25 --padding 2
```

`vision-elements` supports region scoping, repeated `--ignore-region`
rectangles, threshold and component-size controls, `--min-confidence`,
`--max-width`, `--max-height`, `--min-area`, `--max-area`, result `--padding`,
and `--sort position|area|confidence`. `--mask-output` writes the detector
saliency mask and `--overlay-output` writes a screenshot overlay with detected
bounds.

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
JSON output includes a `category` and `severity` for every check plus top-level
`categories` summaries so agents can decide whether capture, input, OCR,
desktop, or Python runtime support is currently usable.

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

For Python, MCP, or other gRPC clients, protect the daemon with `--grpc-token`
or `PEEKABOOX_GRPC_TOKEN`. Python clients, `peekaboox-agent`, and
`peekaboox-mcp` read the same environment variable and expose explicit
`grpc_token` or `--grpc-token` options.
For MCP HTTP/SSE transports, use `peekaboox-mcp --auth-token ...` or
`PEEKABOOX_MCP_TOKEN`; non-loopback HTTP/SSE binds require that token, and
`--max-request-bytes` caps incoming JSON-RPC bodies.

## Plugins

Plugins use the declarative SDK manifest `peekaboox.plugin.json`:

```bash
peekaboox plugins --path examples/plugins
peekaboox plugins --path examples/plugins --json
peekaboox plugin-call org.peekaboox.examples.system-info system_info.uname --path examples/plugins --json
examples/cli/plugins_system_info.sh
```

See [docs/plugins.md](plugins.md) and `examples/plugins/system-info`.
