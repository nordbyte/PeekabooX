#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/drag-absolute-canvas}}"
OUT_FILE="${PEEKABOOX_DRAG_CANVAS_OUTPUT:-$OUT_DIR/peekaboox-drag-canvas.png}"
BEFORE_FILE="$OUT_DIR/before.png"
LOCATE_JSON="$OUT_DIR/canvas-locate.json"
DRY_RUN_JSON="$OUT_DIR/drag-absolute-dry-run.json"
RATIO_DRY_RUN_JSON="$OUT_DIR/drag-ratio-dry-run.json"
AFTER_CAPTURE="$OUT_DIR/after-drag.png"
PAINT_LOG="$OUT_DIR/paint-app.log"
STRICT="${PEEKABOOX_STRICT:-0}"
LIVE="${PEEKABOOX_DRAG_LIVE:-0}"
BACKEND="${PEEKABOOX_DRAG_BACKEND:-auto}"
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

run_step_to_file() {
  local description="$1"
  local file="$2"
  shift 2
  printf '\n== %s ==\n' "$description"
  if "$@" >"$file"; then
    cat "$file"
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
  echo "warning: paint app exited before drag automation could start" >&2
  if [[ -s "$PAINT_LOG" ]]; then
    sed -n '1,20p' "$PAINT_LOG" >&2
  fi
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
  exit 0
}

canvas_geometry() {
  python3 - "$LOCATE_JSON" <<'PY'
import json
import sys

payload = json.loads(open(sys.argv[1], encoding="utf-8").read())
rect = payload.get("rect")
if not rect:
    raise SystemExit("canvas locate result did not include a rectangle")
x = int(rect["x"])
y = int(rect["y"])
w = int(rect["width"])
h = int(rect["height"])
if w <= 0 or h <= 0:
    raise SystemExit(f"canvas rectangle is empty: {x},{y},{w},{h}")
from_x = x + round((w - 1) * 0.18)
from_y = y + round((h - 1) * 0.55)
to_x = x + round((w - 1) * 0.82)
to_y = y + round((h - 1) * 0.55)
print(f"{x},{y},{w},{h} {from_x},{from_y} {to_x},{to_y}")
PY
}

mkdir -p "$OUT_DIR"
create_blank_png
cp "$OUT_FILE" "$BEFORE_FILE"

echo "PeekabooX raw drag canvas example output: $OUT_DIR"
echo "Live mode: $LIVE"

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

run_step "focus paint app" \
  run_peekaboox desktop focus --app "$paint_app" --no-launch --wait-ms "$FOCUS_WAIT_MS"
run_step_to_file "locate paint canvas as JSON" "$LOCATE_JSON" \
  run_peekaboox desktop locate --app "$paint_app" --target canvas --json

if [[ ! -s "$LOCATE_JSON" ]]; then
  run_step "canvas locate JSON unavailable" false
else
  if geometry="$(canvas_geometry)"; then
    read -r canvas_rect from_point to_point <<<"$geometry"

    run_step_to_file "dry-run absolute drag with explicit points" "$DRY_RUN_JSON" \
      run_peekaboox drag --from "$from_point" --to "$to_point" \
        --duration-ms 300 --steps 10 --backend "$BACKEND" --bounds clamp --dry-run --json

    run_step_to_file "dry-run scoped ratio drag" "$RATIO_DRY_RUN_JSON" \
      run_peekaboox drag --region "$canvas_rect" --from-ratio 0.18,0.62 --to-ratio 0.82,0.62 \
        --duration-ms 300 --steps 10 --backend "$BACKEND" --bounds clamp --dry-run --json

    if [[ "$LIVE" == "1" ]]; then
      run_step "live scoped ratio drag with cursor restore" \
        run_peekaboox drag --region "$canvas_rect" --from-ratio 0.18,0.62 --to-ratio 0.82,0.62 \
          --duration-ms 300 --steps 10 --backend "$BACKEND" --bounds clamp --restore
      run_step "capture desktop after live drag" run_peekaboox capture --output "$AFTER_CAPTURE"
    else
      echo
      echo "Dry-run mode only. Set PEEKABOOX_DRAG_LIVE=1 for a real canvas drag."
    fi
  else
    run_step "parse canvas geometry" false
  fi
fi

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX raw drag canvas example passed."
fi
