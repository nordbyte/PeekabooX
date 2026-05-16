#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/click-calculator-keypad}}"
RUN_ID="${PEEKABOOX_CLICK_CALCULATOR_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
WINDOWS_JSON="$OUT_DIR/windows-$RUN_ID.json"
BUTTON_JSON="$OUT_DIR/calculator-button-7-$RUN_ID.json"
SELECTOR_DRY_RUN_JSON="$OUT_DIR/click-selector-button-7-$RUN_ID.json"
COORDINATE_DRY_RUN_JSON="$OUT_DIR/click-coordinate-button-7-$RUN_ID.json"
RATIO_DRY_RUN_JSON="$OUT_DIR/click-window-ratio-$RUN_ID.json"
AFTER_CAPTURE="$OUT_DIR/after-live-click-$RUN_ID.png"
CALCULATOR_LOG="$OUT_DIR/calculator-$RUN_ID.log"
STRICT="${PEEKABOOX_STRICT:-0}"
LIVE="${PEEKABOOX_CLICK_LIVE:-0}"
BACKEND="${PEEKABOOX_CLICK_BACKEND:-auto}"
APP_SCOPE="${PEEKABOOX_CLICK_CALCULATOR_APP_SCOPE:-gnome-calculator}"
WINDOW_TITLE="${PEEKABOOX_CLICK_CALCULATOR_WINDOW_TITLE:-Calculator}"
LAUNCH_DELAY="${PEEKABOOX_CLICK_CALCULATOR_LAUNCH_DELAY:-2}"
BUTTON_7_SELECTOR="role-exact=button,label-exact=7,app=${APP_SCOPE},window-title=${WINDOW_TITLE},min-width=20,min-height=20"
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
  if [[ -n "${PEEKABOOX_CLICK_CALCULATOR_APP:-}" ]]; then
    printf '%s\n' "$PEEKABOOX_CLICK_CALCULATOR_APP"
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
}

extract_center_point() {
  python3 - "$BUTTON_JSON" <<'PY'
import json
import sys

payload = json.loads(open(sys.argv[1], encoding="utf-8").read())
elements = payload.get("elements", payload if isinstance(payload, list) else [])
if not elements:
    raise SystemExit("button lookup returned no elements")
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

require_click_json() {
  local file="$1"
  local label="$2"
  python3 - "$file" "$label" <<'PY'
import json
import sys

payload = json.loads(open(sys.argv[1], encoding="utf-8").read())
label = sys.argv[2]
if not payload.get("ok"):
    raise SystemExit(f"{label}: ok was false")
if not payload.get("dry_run"):
    raise SystemExit(f"{label}: expected dry_run true")
target = payload.get("target") or {}
if "x" not in target or "y" not in target:
    raise SystemExit(f"{label}: target coordinates missing")
if payload.get("requested_backend") is None:
    raise SystemExit(f"{label}: requested_backend missing")
if payload.get("bounds_policy") not in {"allow", "clamp", "fail"}:
    raise SystemExit(f"{label}: invalid bounds_policy")
print(f"{label}: {target['x']},{target['y']} via {payload.get('backend_name')}")
PY
}

write_windows_json() {
  run_peekaboox windows --json >"$WINDOWS_JSON"
}

write_button_json() {
  run_peekaboox elements \
    --app "$APP_SCOPE" \
    --window-title "$WINDOW_TITLE" \
    --role-exact "button" \
    --text-exact "7" \
    --json >"$BUTTON_JSON"
}

write_selector_dry_run_json() {
  run_peekaboox click \
    --selector "$BUTTON_7_SELECTOR" \
    --backend "$BACKEND" \
    --bounds clamp \
    --restore \
    --dry-run \
    --json >"$SELECTOR_DRY_RUN_JSON"
}

write_coordinate_dry_run_json() {
  local point="$1"
  run_peekaboox click \
    --to "$point" \
    --button left \
    --backend "$BACKEND" \
    --bounds clamp \
    --restore \
    --dry-run \
    --json >"$COORDINATE_DRY_RUN_JSON"
}

write_ratio_dry_run_json() {
  run_peekaboox click \
    --app "$APP_SCOPE" \
    --window-title "$WINDOW_TITLE" \
    --ratio 0.5,0.5 \
    --backend "$BACKEND" \
    --bounds clamp \
    --dry-run \
    --json >"$RATIO_DRY_RUN_JSON"
}

run_live_click() {
  run_peekaboox click \
    --selector "$BUTTON_7_SELECTOR" \
    --backend "$BACKEND" \
    --bounds clamp \
    --restore
}

mkdir -p "$OUT_DIR"

for path in "$WINDOWS_JSON" "$BUTTON_JSON" "$SELECTOR_DRY_RUN_JSON" "$COORDINATE_DRY_RUN_JSON" "$RATIO_DRY_RUN_JSON" "$AFTER_CAPTURE" "$CALCULATOR_LOG"; do
  if [[ -e "$path" ]]; then
    echo "error: refusing to overwrite existing file: $path" >&2
    exit 1
  fi
done

if ! calculator_app="$(find_calculator_app)"; then
  skip_or_fail "GNOME Calculator was not found; install gnome-calculator"
fi

echo "PeekabooX click Calculator keypad example output: $OUT_DIR"
echo "Calculator command: $calculator_app"
echo "Live mode: $LIVE"
echo "Click backend: $BACKEND"

launch_calculator_app "$calculator_app"
sleep "$LAUNCH_DELAY"

run_step "focus Calculator window" \
  run_peekaboox desktop focus --app "$APP_SCOPE" --window-title "$WINDOW_TITLE" --no-launch --wait-ms 500
run_step "window enumeration after launching Calculator" write_windows_json
run_step "exact Calculator button lookup" write_button_json

if [[ -s "$BUTTON_JSON" ]]; then
  run_step "semantic selector click dry-run JSON" write_selector_dry_run_json
  if [[ -s "$SELECTOR_DRY_RUN_JSON" ]]; then
    run_step "validate semantic selector click JSON" require_click_json "$SELECTOR_DRY_RUN_JSON" "selector click"
  fi

  if point="$(extract_center_point)"; then
    run_step "coordinate click dry-run JSON" write_coordinate_dry_run_json "$point"
    if [[ -s "$COORDINATE_DRY_RUN_JSON" ]]; then
      run_step "validate coordinate click JSON" require_click_json "$COORDINATE_DRY_RUN_JSON" "coordinate click"
    fi
  else
    failures=$((failures + 1))
    echo "warning: could not extract center point from $BUTTON_JSON" >&2
    [[ "$STRICT" == "1" ]] && exit 1
  fi
fi

run_step "window-scoped ratio click dry-run JSON" write_ratio_dry_run_json
if [[ -s "$RATIO_DRY_RUN_JSON" ]]; then
  run_step "validate window-scoped ratio click JSON" require_click_json "$RATIO_DRY_RUN_JSON" "ratio click"
fi

if [[ "$LIVE" == "1" ]]; then
  run_step "live semantic click on Calculator button 7" run_live_click
  run_step "capture desktop after live click" run_peekaboox capture --output "$AFTER_CAPTURE"
else
  echo
  echo "Dry-run mode only. Set PEEKABOOX_CLICK_LIVE=1 for a real Calculator button click."
fi

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX Calculator click example passed."
fi
