#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/paint-draw}}"
OUT_FILE="${PEEKABOOX_PAINT_OUTPUT:-$OUT_DIR/peekaboox-paint.png}"
BEFORE_FILE="$OUT_DIR/before.png"
AFTER_CAPTURE="$OUT_DIR/after-capture.png"
PAINT_LOG="$OUT_DIR/paint-app.log"
STRICT="${PEEKABOOX_STRICT:-0}"
CANVAS_X="${PEEKABOOX_PAINT_CANVAS_X:-360}"
CANVAS_Y="${PEEKABOOX_PAINT_CANVAS_Y:-360}"
STROKE_W="${PEEKABOOX_PAINT_STROKE_W:-260}"
STROKE_H="${PEEKABOOX_PAINT_STROKE_H:-160}"
SAVE_X="${PEEKABOOX_PAINT_SAVE_X:-285}"
SAVE_Y="${PEEKABOOX_PAINT_SAVE_Y:-181}"
LAUNCH_DELAY="${PEEKABOOX_PAINT_LAUNCH_DELAY:-3}"
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
echo "Canvas origin: $CANVAS_X,$CANVAS_Y"
echo "Save button: $SAVE_X,$SAVE_Y"

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
run_step "move pointer to canvas" run_peekaboox move --x "$CANVAS_X" --y "$CANVAS_Y"
run_step "draw horizontal stroke" \
  run_peekaboox drag --from "$CANVAS_X,$CANVAS_Y" --to "$((CANVAS_X + STROKE_W)),$CANVAS_Y" --duration-ms 250
run_step "draw diagonal stroke" \
  run_peekaboox drag --from "$((CANVAS_X + 20)),$((CANVAS_Y + 40))" --to "$((CANVAS_X + STROKE_W)),$((CANVAS_Y + STROKE_H))" --duration-ms 350
run_step "draw vertical stroke" \
  run_peekaboox drag --from "$((CANVAS_X + 80)),$((CANVAS_Y + 20))" --to "$((CANVAS_X + 80)),$((CANVAS_Y + STROKE_H))" --duration-ms 250
run_step "save drawing with ctrl+s" run_peekaboox hotkey ctrl+s
sleep 1
if cmp -s "$BEFORE_FILE" "$OUT_FILE"; then
  run_step "save drawing from toolbar" run_peekaboox click --x "$SAVE_X" --y "$SAVE_Y"
fi
sleep 1
run_step "capture desktop after drawing" run_peekaboox capture --output "$AFTER_CAPTURE"

if cmp -s "$BEFORE_FILE" "$OUT_FILE"; then
  failures=$((failures + 1))
  echo "warning: output file did not change after save: $OUT_FILE" >&2
  echo "         adjust PEEKABOOX_PAINT_CANVAS_X/Y or run with an XWayland-capable paint app" >&2
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
