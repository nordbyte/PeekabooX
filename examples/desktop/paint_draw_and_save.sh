#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/paint-draw}}"
OUT_FILE="${PEEKABOOX_PAINT_OUTPUT:-$OUT_DIR/peekaboox-paint.png}"
BEFORE_FILE="$OUT_DIR/before.png"
AFTER_CAPTURE="$OUT_DIR/after-capture.png"
PAINT_LOG="$OUT_DIR/paint-app.log"
STRICT="${PEEKABOOX_STRICT:-0}"
LAUNCH_DELAY="${PEEKABOOX_PAINT_LAUNCH_DELAY:-3}"
FOCUS_WAIT_MS="${PEEKABOOX_PAINT_FOCUS_WAIT_MS:-500}"
failures=0
paint_pid=""

run_peekaboox() {
  if [[ -n "${PEEKABOOX_BIN:-}" ]]; then
    "$PEEKABOOX_BIN" "$@"
  elif command -v peekaboox >/dev/null 2>&1; then
    peekaboox "$@"
  else
    cargo run --quiet -p peekaboox-cli -- "$@"
  fi
}

run_step() {
  local description="$1"
  shift
  printf '\n== %s ==\n' "$description"
  if "$@"; then
    return 0
  fi

  failures=$((failures + 1))
  if [[ "$STRICT" == "1" ]]; then
    echo "failed: $description" >&2
    exit 1
  fi
  echo "warning: $description failed in this desktop environment" >&2
  return 0
}

create_blank_png() {
  python3 - "$OUT_FILE" <<'PY'
import struct
import sys
import zlib
from pathlib import Path

path = Path(sys.argv[1])
width = 800
height = 600
row = b"\x00" + (b"\xff\xff\xff" * width)
raw = row * height

def chunk(kind: bytes, payload: bytes) -> bytes:
    return (
        struct.pack(">I", len(payload))
        + kind
        + payload
        + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
    )

png = (
    b"\x89PNG\r\n\x1a\n"
    + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    + chunk(b"IDAT", zlib.compress(raw, 9))
    + chunk(b"IEND", b"")
)
path.parent.mkdir(parents=True, exist_ok=True)
path.write_bytes(png)
PY
}

find_paint_app() {
  if [[ -n "${PEEKABOOX_PAINT_APP:-}" ]]; then
    printf '%s\n' "$PEEKABOOX_PAINT_APP"
    return 0
  fi

  local candidate
  for candidate in drawing pinta kolourpaint; do
    if command -v "$candidate" >/dev/null 2>&1; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  return 1
}

launch_paint_app() {
  local app="$1"
  shift

  if [[ "${XDG_SESSION_TYPE:-}" == "wayland" && "${PEEKABOOX_PAINT_FORCE_XWAYLAND:-0}" == "1" ]]; then
    GDK_BACKEND=x11 QT_QPA_PLATFORM=xcb "$app" "$@" >"$PAINT_LOG" 2>&1 &
  else
    "$app" "$@" >"$PAINT_LOG" 2>&1 &
  fi
  paint_pid="$!"
}

ensure_paint_app_started() {
  if [[ -z "$paint_pid" ]] || kill -0 "$paint_pid" >/dev/null 2>&1; then
    return 0
  fi

  failures=$((failures + 1))
  echo "warning: paint app exited before input automation could start" >&2
  if [[ -s "$PAINT_LOG" ]]; then
    sed -n '1,20p' "$PAINT_LOG" >&2
  fi
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
  exit 0
}

maybe_close_paint_app() {
  if [[ "${PEEKABOOX_PAINT_CLOSE:-0}" == "1" ]]; then
    run_step "close paint app" run_peekaboox hotkey Alt+F4
  fi
}

mkdir -p "$OUT_DIR"
create_blank_png
cp "$OUT_FILE" "$BEFORE_FILE"

echo "PeekabooX paint example output: $OUT_DIR"
echo "Canvas target: desktop paint/canvas"

if ! paint_app="$(find_paint_app)"; then
  echo "warning: no supported paint app found; install drawing, pinta, or kolourpaint" >&2
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
  exit 0
fi

echo "Launching paint app: $paint_app"
launch_paint_app "$paint_app" "$OUT_FILE"
sleep "$LAUNCH_DELAY"
ensure_paint_app_started

run_step "window enumeration after launch" run_peekaboox windows
run_step "focus paint app" run_peekaboox desktop focus --app "$paint_app" --no-launch --wait-ms "$FOCUS_WAIT_MS"
run_step "locate paint canvas" run_peekaboox desktop locate --app "$paint_app" --target canvas
run_step "draw horizontal stroke" \
  run_peekaboox desktop drag --app "$paint_app" --target canvas --from-ratio 0.22,0.30 --to-ratio 0.70,0.30 --duration-ms 250
run_step "draw diagonal stroke" \
  run_peekaboox desktop drag --app "$paint_app" --target canvas --from-ratio 0.25,0.40 --to-ratio 0.72,0.66 --duration-ms 350
run_step "draw vertical stroke" \
  run_peekaboox desktop drag --app "$paint_app" --target canvas --from-ratio 0.36,0.34 --to-ratio 0.36,0.68 --duration-ms 250
run_step "save drawing with ctrl+s" run_peekaboox hotkey ctrl+s
sleep 1
if cmp -s "$BEFORE_FILE" "$OUT_FILE"; then
  run_step "save drawing from toolbar" run_peekaboox desktop click --app "$paint_app" --target save-button
fi
sleep 1
run_step "capture desktop after drawing" run_peekaboox capture --output "$AFTER_CAPTURE"

if cmp -s "$BEFORE_FILE" "$OUT_FILE"; then
  failures=$((failures + 1))
  echo "warning: output file did not change after save: $OUT_FILE" >&2
  echo "         run with PEEKABOOX_STRICT=1 for a failing smoke test, or try an XWayland-capable paint app" >&2
  if [[ "$STRICT" == "1" ]]; then
    maybe_close_paint_app
    exit 1
  fi
else
  echo "Saved changed drawing: $OUT_FILE"
fi

maybe_close_paint_app

if [[ -n "$paint_pid" ]] && ! kill -0 "$paint_pid" >/dev/null 2>&1; then
  paint_pid=""
fi

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX paint draw-and-save example passed."
fi
