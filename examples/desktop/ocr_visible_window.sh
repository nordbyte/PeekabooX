#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/desktop-ocr}}"
RUN_ID="${PEEKABOOX_DESKTOP_OCR_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
SAMPLE_IMAGE="${PEEKABOOX_DESKTOP_OCR_SAMPLE:-$ROOT/examples/desktop/assets/ocr_desktop_sample.png}"
CAPTURE_FILE="$OUT_DIR/desktop-ocr-capture-$RUN_ID.png"
OCR_TEXT_FILE="$OUT_DIR/desktop-ocr-text-$RUN_ID.txt"
OCR_JSON_FILE="$OUT_DIR/desktop-ocr-result-$RUN_ID.json"
VIEWER_LOG="$OUT_DIR/desktop-ocr-viewer-$RUN_ID.log"
STRICT="${PEEKABOOX_STRICT:-0}"
OPEN_SAMPLE="${PEEKABOOX_DESKTOP_OCR_OPEN:-1}"
OPEN_DELAY="${PEEKABOOX_DESKTOP_OCR_OPEN_DELAY:-3}"
OCR_LANGUAGE="${PEEKABOOX_DESKTOP_OCR_LANGUAGE:-eng}"
OCR_PSM="${PEEKABOOX_DESKTOP_OCR_PSM:-11}"
OCR_MIN_CONFIDENCE="${PEEKABOOX_DESKTOP_OCR_MIN_CONFIDENCE:-0.20}"
failures=0
viewer_pid=""

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

open_sample_image() {
  if [[ "$OPEN_SAMPLE" != "1" ]]; then
    echo "Skipping sample image launch because PEEKABOOX_DESKTOP_OCR_OPEN=$OPEN_SAMPLE"
    return 0
  fi

  if [[ -n "${PEEKABOOX_DESKTOP_OCR_VIEWER:-}" ]]; then
    "$PEEKABOOX_DESKTOP_OCR_VIEWER" "$SAMPLE_IMAGE" >"$VIEWER_LOG" 2>&1 &
    viewer_pid="$!"
    return 0
  fi

  if command -v xdg-open >/dev/null 2>&1; then
    xdg-open "$SAMPLE_IMAGE" >"$VIEWER_LOG" 2>&1 &
    viewer_pid="$!"
    return 0
  fi

  if command -v gio >/dev/null 2>&1; then
    gio open "$SAMPLE_IMAGE" >"$VIEWER_LOG" 2>&1 &
    viewer_pid="$!"
    return 0
  fi

  return 1
}

ocr_target_args() {
  if [[ -n "${PEEKABOOX_DESKTOP_OCR_WINDOW_ID:-}" ]]; then
    printf '%s\0%s\0' "--window-id" "$PEEKABOOX_DESKTOP_OCR_WINDOW_ID"
  elif [[ -n "${PEEKABOOX_DESKTOP_OCR_WINDOW_TITLE:-}" ]]; then
    printf '%s\0%s\0' "--window-title" "$PEEKABOOX_DESKTOP_OCR_WINDOW_TITLE"
  elif [[ -n "${PEEKABOOX_DESKTOP_OCR_APP:-}" ]]; then
    printf '%s\0%s\0' "--app" "$PEEKABOOX_DESKTOP_OCR_APP"
  fi
}

require_ocr_text() {
  local description="$1"
  local pattern="$2"
  local output="$3"
  if grep -Eiq "$pattern" <<<"$output"; then
    return 0
  fi

  failures=$((failures + 1))
  echo "warning: OCR output did not contain expected text for $description" >&2
  echo "pattern: $pattern" >&2
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
}

write_ocr_json_result() {
  run_peekaboox ocr \
    "${target_args[@]}" \
    --language "$OCR_LANGUAGE" \
    --psm "$OCR_PSM" \
    --min-confidence "$OCR_MIN_CONFIDENCE" \
    --scale 2 \
    --grayscale \
    --json >"$OCR_JSON_FILE"
}

mkdir -p "$OUT_DIR"

if [[ ! -f "$SAMPLE_IMAGE" ]]; then
  echo "error: missing OCR desktop sample image: $SAMPLE_IMAGE" >&2
  exit 1
fi

if ! command -v tesseract >/dev/null 2>&1; then
  skip_or_fail "tesseract is not available; install tesseract-ocr for desktop OCR"
fi

echo "PeekabooX desktop OCR example output: $OUT_DIR"
echo "Sample image: $SAMPLE_IMAGE"
echo "OCR language: $OCR_LANGUAGE"

if ! open_sample_image; then
  skip_or_fail "no desktop opener found; install xdg-utils or set PEEKABOOX_DESKTOP_OCR_VIEWER"
fi

if [[ "$OPEN_SAMPLE" == "1" ]]; then
  echo "Waiting ${OPEN_DELAY}s for the image viewer to become visible"
  sleep "$OPEN_DELAY"
fi

run_step "window enumeration after opening OCR sample" run_peekaboox windows
run_step "capture desktop with visible OCR sample" run_peekaboox capture --output "$CAPTURE_FILE"

target_args=()
while IFS= read -r -d '' value; do
  target_args+=("$value")
done < <(ocr_target_args)

printf '\n== live desktop OCR ==\n'
if ocr_text="$(
  run_peekaboox ocr \
    "${target_args[@]}" \
    --language "$OCR_LANGUAGE" \
    --psm "$OCR_PSM" \
    --min-confidence "$OCR_MIN_CONFIDENCE" \
    --scale 2 \
    --grayscale \
    --words
)"; then
  printf '%s\n' "$ocr_text" | tee "$OCR_TEXT_FILE"
else
  failures=$((failures + 1))
  echo "warning: live desktop OCR failed" >&2
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
  ocr_text=""
fi

if [[ -n "$ocr_text" ]]; then
  require_ocr_text "ticket id" "PX[-[:space:]]*OCR[-[:space:]]*204" "$ocr_text"
  require_ocr_text "ready status" "READY" "$ocr_text"
  require_ocr_text "verification action" "VERIFY[[:space:]]+SCREEN[[:space:]]+TEXT" "$ocr_text"
fi

run_step "write OCR JSON result" write_ocr_json_result

if [[ -n "$viewer_pid" ]] && ! kill -0 "$viewer_pid" >/dev/null 2>&1; then
  viewer_pid=""
fi

echo
echo "Capture: $CAPTURE_FILE"
echo "OCR text: $OCR_TEXT_FILE"
echo "OCR JSON: $OCR_JSON_FILE"

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX desktop OCR example passed."
fi
