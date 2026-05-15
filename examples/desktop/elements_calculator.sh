#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/elements-calculator}}"
RUN_ID="${PEEKABOOX_ELEMENTS_CALCULATOR_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
WINDOWS_JSON="$OUT_DIR/windows-$RUN_ID.json"
SCOPED_JSON="$OUT_DIR/calculator-elements-$RUN_ID.json"
DIGIT_JSON="$OUT_DIR/calculator-digit-buttons-$RUN_ID.json"
BUTTON_JSON="$OUT_DIR/calculator-button-7-$RUN_ID.json"
CONTAINS_JSON="$OUT_DIR/calculator-contains-button-7-$RUN_ID.json"
VISION_JSON="$OUT_DIR/calculator-vision-fallback-$RUN_ID.json"
CLICK_DRY_RUN_TXT="$OUT_DIR/calculator-click-button-7-dry-run-$RUN_ID.txt"
CALCULATOR_LOG="$OUT_DIR/calculator-$RUN_ID.log"
STRICT="${PEEKABOOX_STRICT:-0}"
APP_SCOPE="${PEEKABOOX_ELEMENTS_CALCULATOR_APP_SCOPE:-gnome-calculator}"
WINDOW_TITLE="${PEEKABOOX_ELEMENTS_CALCULATOR_WINDOW_TITLE:-Calculator}"
LAUNCH_DELAY="${PEEKABOOX_ELEMENTS_CALCULATOR_LAUNCH_DELAY:-2}"
ELEMENT_LIMIT="${PEEKABOOX_ELEMENTS_CALCULATOR_LIMIT:-120}"
BUTTON_7_SELECTOR="role-exact=button,label-exact=7,app=${APP_SCOPE},window-title=${WINDOW_TITLE},min-width=20,min-height=20"
calculator_pid=""
failures=0

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
  if [[ -n "${PEEKABOOX_ELEMENTS_CALCULATOR_APP:-}" ]]; then
    printf '%s\n' "$PEEKABOOX_ELEMENTS_CALCULATOR_APP"
    return 0
  fi

  if command -v gnome-calculator >/dev/null 2>&1; then
    printf '%s\n' "gnome-calculator"
    return 0
  fi

  return 1
}

launch_calculator_app() {
  local app="$1"
  "$app" >"$CALCULATOR_LOG" 2>&1 &
  calculator_pid="$!"
}

maybe_close_calculator() {
  if [[ "${PEEKABOOX_ELEMENTS_CALCULATOR_CLOSE:-0}" == "1" ]] \
    && [[ -n "$calculator_pid" ]] \
    && kill -0 "$calculator_pid" >/dev/null 2>&1; then
    kill "$calculator_pid" >/dev/null 2>&1 || true
    wait "$calculator_pid" >/dev/null 2>&1 || true
  fi
}
trap maybe_close_calculator EXIT

json_count() {
  python3 - "$1" <<'PY'
import json
import sys

payload = json.loads(open(sys.argv[1], encoding="utf-8").read())
elements = payload.get("elements", payload if isinstance(payload, list) else [])
print(len(elements))
PY
}

require_json_min() {
  local file="$1"
  local minimum="$2"
  local label="$3"
  local count
  count="$(json_count "$file")" || return 1
  if (( count < minimum )); then
    echo "expected at least $minimum element(s) in $label, got $count" >&2
    return 1
  fi
  echo "$label: $count element(s)"
}

require_window_title_metadata() {
  python3 - "$1" "$WINDOW_TITLE" <<'PY'
import json
import sys

payload = json.loads(open(sys.argv[1], encoding="utf-8").read())
title = sys.argv[2].casefold()
elements = payload.get("elements", payload if isinstance(payload, list) else [])
matches = [
    element
    for element in elements
    if title in str(element.get("window_title") or "").casefold()
]
if not matches:
    raise SystemExit("no element carried the expected window_title metadata")
print(f"window_title metadata: {len(matches)} element(s)")
PY
}

require_exact_button_label() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

payload = json.loads(open(sys.argv[1], encoding="utf-8").read())
label = sys.argv[2]
elements = payload.get("elements", payload if isinstance(payload, list) else [])
if not any(element.get("role") == "button" and element.get("label") == label for element in elements):
    raise SystemExit(f"button {label!r} was not found")
print(f"button {label!r} found")
PY
}

require_dry_run_click_output() {
  if ! grep -Eq '^would click selector .*label-exact=7.* at -?[0-9]+,-?[0-9]+ \(7\) via .+$' "$1"; then
    echo "dry-run output did not describe the expected semantic click" >&2
    return 1
  fi
  cat "$1"
}

extract_center_point() {
  python3 - "$BUTTON_JSON" <<'PY'
import json
import sys

payload = json.loads(open(sys.argv[1], encoding="utf-8").read())
elements = payload.get("elements", payload if isinstance(payload, list) else [])
if not elements:
    raise SystemExit("no elements in button JSON output")
center = elements[0].get("center")
if center is None:
    bounds = elements[0]["bounds"]
    center = {
        "x": bounds["x"] + bounds["width"] // 2,
        "y": bounds["y"] + bounds["height"] // 2,
    }
print(f"{center['x']},{center['y']}")
PY
}

write_windows_json() {
  run_peekaboox windows --json >"$WINDOWS_JSON"
}

write_scoped_json() {
  run_peekaboox elements \
    --app "$APP_SCOPE" \
    --window-title "$WINDOW_TITLE" \
    --limit "$ELEMENT_LIMIT" \
    --json >"$SCOPED_JSON"
}

write_digit_json() {
  run_peekaboox elements \
    --app "$APP_SCOPE" \
    --window-title "$WINDOW_TITLE" \
    --selector "role-exact=button,label-regex=^[0-9]$,state=focusable,not-state=disabled,min-width=20,min-height=20" \
    --json >"$DIGIT_JSON"
}

write_button_json() {
  run_peekaboox elements \
    --app "$APP_SCOPE" \
    --window-title "$WINDOW_TITLE" \
    --role-exact "button" \
    --text-exact "7" \
    --json >"$BUTTON_JSON"
}

write_click_dry_run() {
  run_peekaboox click \
    --selector "$BUTTON_7_SELECTOR" \
    --dry-run >"$CLICK_DRY_RUN_TXT"
}

write_contains_json() {
  local point="$1"
  run_peekaboox elements \
    --app "$APP_SCOPE" \
    --window-title "$WINDOW_TITLE" \
    --contains "$point" \
    --json >"$CONTAINS_JSON"
}

write_vision_json() {
  run_peekaboox elements \
    --app "$APP_SCOPE" \
    --window-title "$WINDOW_TITLE" \
    --selector "role=visual-region,min-width=20,min-height=20" \
    --vision-fallback \
    --vision-threshold 24 \
    --vision-max-elements 40 \
    --json >"$VISION_JSON"
}

mkdir -p "$OUT_DIR"

for path in "$WINDOWS_JSON" "$SCOPED_JSON" "$DIGIT_JSON" "$BUTTON_JSON" "$CONTAINS_JSON" "$VISION_JSON" "$CLICK_DRY_RUN_TXT" "$CALCULATOR_LOG"; do
  if [[ -e "$path" ]]; then
    echo "error: refusing to overwrite existing file: $path" >&2
    exit 1
  fi
done

if ! calculator_app="$(find_calculator_app)"; then
  skip_or_fail "GNOME Calculator was not found; install gnome-calculator"
fi

echo "PeekabooX real elements example output: $OUT_DIR"
echo "Calculator command: $calculator_app"
echo "Element app scope: $APP_SCOPE"
echo "Window title scope: $WINDOW_TITLE"
echo "Button 7 selector: $BUTTON_7_SELECTOR"

launch_calculator_app "$calculator_app"
sleep "$LAUNCH_DELAY"

run_step "window enumeration after launching Calculator" write_windows_json

run_step "scoped Calculator element snapshot" write_scoped_json
if [[ -s "$SCOPED_JSON" ]]; then
  run_step "validate scoped Calculator elements" require_json_min "$SCOPED_JSON" 20 "Calculator scoped snapshot"
  run_step "validate window title metadata" require_window_title_metadata "$SCOPED_JSON"
fi

run_step "digit button selector" write_digit_json
if [[ -s "$DIGIT_JSON" ]]; then
  run_step "validate digit button selector" require_json_min "$DIGIT_JSON" 10 "Calculator digit buttons"
fi

run_step "exact button lookup" write_button_json
if [[ -s "$BUTTON_JSON" ]]; then
  run_step "validate exact button lookup" require_exact_button_label "$BUTTON_JSON" "7"
  run_step "semantic click dry-run from exact selector" write_click_dry_run
  if [[ -s "$CLICK_DRY_RUN_TXT" ]]; then
    run_step "validate semantic click dry-run output" require_dry_run_click_output "$CLICK_DRY_RUN_TXT"
  fi
  if point="$(extract_center_point)"; then
    run_step "contains selector from button center point" write_contains_json "$point"
    if [[ -s "$CONTAINS_JSON" ]]; then
      run_step "validate contains selector output" require_json_min "$CONTAINS_JSON" 1 "Calculator contains lookup"
    fi
  else
    failures=$((failures + 1))
    echo "warning: could not extract center point from $BUTTON_JSON" >&2
    [[ "$STRICT" == "1" ]] && exit 1
  fi
fi

run_step "scoped vision fallback on Calculator window" write_vision_json
if [[ -s "$VISION_JSON" ]]; then
  run_step "validate scoped vision fallback output" require_json_min "$VISION_JSON" 1 "Calculator vision fallback"
fi

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX Calculator elements example passed."
fi
