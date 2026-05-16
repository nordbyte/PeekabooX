#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/capture-window-targets}}"
RUN_ID="${PEEKABOOX_CAPTURE_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
APP_QUERY="${PEEKABOOX_CAPTURE_APP_QUERY:-calculator}"
WINDOW_TITLE_REGEX="${PEEKABOOX_CAPTURE_TITLE_REGEX:-Calculator}"
LAUNCH_DELAY="${PEEKABOOX_CAPTURE_LAUNCH_DELAY:-2}"
STRICT="${PEEKABOOX_STRICT:-0}"

WINDOWS_JSON="$OUT_DIR/windows-$RUN_ID.json"
APP_CAPTURE_JSON="$OUT_DIR/capture-app-$RUN_ID.json"
APP_CAPTURE_PNG="$OUT_DIR/capture-app-$RUN_ID.png"
REGION_CAPTURE_JSON="$OUT_DIR/capture-window-region-$RUN_ID.json"
REGION_CAPTURE_PNG="$OUT_DIR/capture-window-region-$RUN_ID.png"
SEMANTIC_CAPTURE_JSON="$OUT_DIR/capture-semantic-$RUN_ID.json"
SEMANTIC_CAPTURE_PNG="$OUT_DIR/capture-semantic-$RUN_ID.png"
STDOUT_PNG="$OUT_DIR/capture-stdout-$RUN_ID.png"
XWD_CAPTURE_JSON="$OUT_DIR/capture-xwd-$RUN_ID.json"
XWD_CAPTURE="$OUT_DIR/capture-xwd-$RUN_ID.xwd"
NO_OVERWRITE_TARGET="$OUT_DIR/capture-no-overwrite-$RUN_ID.png"

failures=0
launched_pid=""

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

skip_or_fail() {
  local message="$1"
  if [[ "$STRICT" == "1" ]]; then
    echo "error: $message" >&2
    exit 1
  fi
  echo "warning: $message" >&2
  exit 0
}

find_calculator_app() {
  if [[ -n "${PEEKABOOX_CAPTURE_APP:-}" ]]; then
    printf '%s\n' "$PEEKABOOX_CAPTURE_APP"
    return 0
  fi
  if command -v gnome-calculator >/dev/null 2>&1; then
    printf '%s\n' "gnome-calculator"
    return 0
  fi
  if command -v kcalc >/dev/null 2>&1; then
    printf '%s\n' "kcalc"
    return 0
  fi
  if command -v galculator >/dev/null 2>&1; then
    printf '%s\n' "galculator"
    return 0
  fi
  return 1
}

write_windows_json() {
  run_peekaboox windows --json --app "$APP_QUERY" --title-regex "$WINDOW_TITLE_REGEX" >"$WINDOWS_JSON"
}

first_window_id() {
  python3 - "$WINDOWS_JSON" <<'PY'
import json
import sys

payload = json.load(open(sys.argv[1], encoding="utf-8"))
windows = payload.get("windows") or []
if not windows:
    raise SystemExit(1)
print(windows[0]["id"])
PY
}

ensure_window() {
  write_windows_json && first_window_id >/dev/null && return 0

  local app
  app="$(find_calculator_app)" || skip_or_fail "no calculator app found; set PEEKABOOX_CAPTURE_APP"
  "$app" >/dev/null 2>&1 &
  launched_pid="$!"
  sleep "$LAUNCH_DELAY"
  write_windows_json
  first_window_id >/dev/null
}

validate_capture_json() {
  local json_file="$1"
  local image_file="$2"
  local mime_type="$3"
  local allow_zero_size="${4:-0}"
  python3 - "$json_file" "$image_file" "$mime_type" "$allow_zero_size" <<'PY'
import json
import os
import sys

payload = json.load(open(sys.argv[1], encoding="utf-8"))
image_file = sys.argv[2]
mime_type = sys.argv[3]
allow_zero_size = sys.argv[4] == "1"
if payload.get("mime_type") != mime_type:
    raise SystemExit(f"unexpected mime type: {payload.get('mime_type')}")
if payload.get("bytes_written", 0) <= 0:
    raise SystemExit("capture reported no bytes")
if not allow_zero_size and (payload.get("width", 0) <= 0 or payload.get("height", 0) <= 0):
    raise SystemExit("PNG capture reported empty dimensions")
if not os.path.exists(image_file) or os.path.getsize(image_file) <= 0:
    raise SystemExit(f"missing capture output: {image_file}")
if payload.get("output_path") and os.path.abspath(image_file) != payload["output_path"]:
    raise SystemExit("capture output_path does not match requested file")
if "captured_at_unix_ms" not in payload or "source" not in payload:
    raise SystemExit("capture metadata is incomplete")
PY
}

validate_png_file() {
  python3 - "$1" <<'PY'
import sys

with open(sys.argv[1], "rb") as handle:
    signature = handle.read(8)
if signature != b"\x89PNG\r\n\x1a\n":
    raise SystemExit("not a PNG file")
PY
}

capture_by_app_json() {
  run_peekaboox capture \
    --json \
    --app "$APP_QUERY" \
    --title-regex "$WINDOW_TITLE_REGEX" \
    --output "$APP_CAPTURE_PNG" >"$APP_CAPTURE_JSON"
}

capture_window_relative_region_json() {
  local window_id
  window_id="$(first_window_id)"
  run_peekaboox capture \
    --json \
    --window-id "$window_id" \
    --region 10,10,220,160 \
    --output "$REGION_CAPTURE_PNG" >"$REGION_CAPTURE_JSON"
}

capture_with_semantic_tree_json() {
  run_peekaboox capture \
    --json \
    --include-semantic-tree \
    --app "$APP_QUERY" \
    --title-regex "$WINDOW_TITLE_REGEX" \
    --output "$SEMANTIC_CAPTURE_PNG" >"$SEMANTIC_CAPTURE_JSON"
}

capture_stdout_png() {
  run_peekaboox capture --stdout >"$STDOUT_PNG"
}

capture_xwd_json() {
  run_peekaboox capture --json --format xwd --output "$XWD_CAPTURE" >"$XWD_CAPTURE_JSON"
}

no_overwrite_rejects_existing_file() {
  printf 'existing\n' >"$NO_OVERWRITE_TARGET"
  if run_peekaboox capture --output "$NO_OVERWRITE_TARGET" --no-overwrite >/dev/null 2>&1; then
    echo "capture unexpectedly overwrote existing file" >&2
    return 1
  fi
}

mkdir -p "$OUT_DIR"

echo "PeekabooX capture target example output: $OUT_DIR"
run_step "ensure target window" ensure_window
run_step "capture target window by app/title-regex" capture_by_app_json
run_step "validate app/title capture" validate_capture_json "$APP_CAPTURE_JSON" "$APP_CAPTURE_PNG" image/png
run_step "capture window-relative region by window id" capture_window_relative_region_json
run_step "validate window-relative region capture" validate_capture_json "$REGION_CAPTURE_JSON" "$REGION_CAPTURE_PNG" image/png
run_step "capture target with semantic tree metadata" capture_with_semantic_tree_json
run_step "validate semantic capture metadata" validate_capture_json "$SEMANTIC_CAPTURE_JSON" "$SEMANTIC_CAPTURE_PNG" image/png
run_step "capture PNG to stdout" capture_stdout_png
run_step "validate stdout PNG" validate_png_file "$STDOUT_PNG"
run_step "capture full screen as XWD" capture_xwd_json
run_step "validate XWD metadata" validate_capture_json "$XWD_CAPTURE_JSON" "$XWD_CAPTURE" image/x-xwindowdump 1
run_step "verify no-overwrite guard" no_overwrite_rejects_existing_file

if [[ -n "$launched_pid" && "${PEEKABOOX_CAPTURE_CLOSE_APP:-0}" == "1" ]]; then
  kill "$launched_pid" >/dev/null 2>&1 || true
fi

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX capture target example passed."
fi
