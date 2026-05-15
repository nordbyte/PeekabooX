#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/windows-inventory}}"
RUN_ID="${PEEKABOOX_WINDOWS_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
ALL_WINDOWS_JSON="$OUT_DIR/windows-all-$RUN_ID.json"
FOCUSED_JSON="$OUT_DIR/windows-focused-$RUN_ID.json"
CALCULATOR_JSON="$OUT_DIR/windows-calculator-$RUN_ID.json"
CALCULATOR_BY_ID_JSON="$OUT_DIR/windows-calculator-by-id-$RUN_ID.json"
CALCULATOR_ELEMENTS_JSON="$OUT_DIR/windows-calculator-elements-$RUN_ID.json"
CALCULATOR_CAPTURE="$OUT_DIR/windows-calculator-$RUN_ID.png"
CALCULATOR_ID_TXT="$OUT_DIR/windows-calculator-id-$RUN_ID.txt"
CALCULATOR_LOG="$OUT_DIR/calculator-$RUN_ID.log"
STRICT="${PEEKABOOX_STRICT:-0}"
WINDOWS_BACKEND="${PEEKABOOX_WINDOWS_BACKEND:-auto}"
APP_QUERY="${PEEKABOOX_WINDOWS_APP_QUERY:-calculator}"
WINDOW_TITLE="${PEEKABOOX_WINDOWS_TITLE:-Calculator}"
WINDOW_TITLE_REGEX="${PEEKABOOX_WINDOWS_TITLE_REGEX:-Calculator}"
LAUNCH_DELAY="${PEEKABOOX_WINDOWS_LAUNCH_DELAY:-2}"
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
  if [[ -n "${PEEKABOOX_WINDOWS_CALCULATOR_APP:-}" ]]; then
    printf '%s\n' "$PEEKABOOX_WINDOWS_CALCULATOR_APP"
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
  if [[ "${PEEKABOOX_WINDOWS_CLOSE:-0}" == "1" ]] \
    && [[ -n "$calculator_pid" ]] \
    && kill -0 "$calculator_pid" >/dev/null 2>&1; then
    kill "$calculator_pid" >/dev/null 2>&1 || true
    wait "$calculator_pid" >/dev/null 2>&1 || true
  fi
}
trap maybe_close_calculator EXIT

write_all_windows_json() {
  run_peekaboox windows \
    --backend "$WINDOWS_BACKEND" \
    --sort focused \
    --diagnose \
    --json >"$ALL_WINDOWS_JSON"
}

write_focused_window_json() {
  run_peekaboox windows \
    --backend "$WINDOWS_BACKEND" \
    --focused \
    --limit 1 \
    --sort focused \
    --json >"$FOCUSED_JSON"
}

write_calculator_window_json() {
  run_peekaboox windows \
    --backend "$WINDOWS_BACKEND" \
    --app "$APP_QUERY" \
    --title-regex "$WINDOW_TITLE_REGEX" \
    --limit 1 \
    --sort focused \
    --diagnose \
    --json >"$CALCULATOR_JSON"
}

write_calculator_by_id_json() {
  local window_id="$1"
  run_peekaboox windows \
    --backend "$WINDOWS_BACKEND" \
    --id "$window_id" \
    --json >"$CALCULATOR_BY_ID_JSON"
}

capture_calculator_window() {
  local window_id="$1"
  run_peekaboox capture --window-id "$window_id" --output "$CALCULATOR_CAPTURE"
}

write_calculator_elements_json() {
  local window_id="$1"
  run_peekaboox elements \
    --window-id "$window_id" \
    --limit 40 \
    --json >"$CALCULATOR_ELEMENTS_JSON"
}

require_window_result() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

path, label = sys.argv[1:3]
payload = json.loads(open(path, encoding="utf-8").read())
windows = payload.get("windows", payload if isinstance(payload, list) else [])
if not windows:
    raise SystemExit(f"{label}: no windows returned")
if not payload.get("backend_name"):
    raise SystemExit(f"{label}: missing backend_name")
if "backend_reports" in payload and not payload["backend_reports"]:
    raise SystemExit(f"{label}: missing backend diagnostics")
print(f"{label}: {len(windows)} window(s) via {payload.get('backend_name', 'unknown')}")
PY
}

require_at_most_one_window() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

path, label = sys.argv[1:3]
payload = json.loads(open(path, encoding="utf-8").read())
windows = payload.get("windows", payload if isinstance(payload, list) else [])
if len(windows) > 1:
    raise SystemExit(f"{label}: expected at most one window, got {len(windows)}")
print(f"{label}: {len(windows)} focused window(s)")
PY
}

extract_first_window_id() {
  python3 - "$1" <<'PY'
import json
import sys

payload = json.loads(open(sys.argv[1], encoding="utf-8").read())
windows = payload.get("windows", payload if isinstance(payload, list) else [])
for window in windows:
    bounds = window.get("bounds") or {}
    if bounds.get("width", 0) > 0 and bounds.get("height", 0) > 0:
        print(window["id"])
        raise SystemExit(0)
raise SystemExit("no window with non-empty bounds found")
PY
}

require_same_window_id() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

path, expected = sys.argv[1:3]
payload = json.loads(open(path, encoding="utf-8").read())
windows = payload.get("windows", payload if isinstance(payload, list) else [])
if len(windows) != 1:
    raise SystemExit(f"expected exactly one id-filtered window, got {len(windows)}")
if windows[0].get("id") != expected:
    raise SystemExit(f"expected window id {expected!r}, got {windows[0].get('id')!r}")
print(f"id filter resolved {expected}")
PY
}

mkdir -p "$OUT_DIR"

echo "PeekabooX windows inventory output: $OUT_DIR"
run_step "full window inventory with diagnostics" write_all_windows_json
run_step "validate window inventory metadata" require_window_result "$ALL_WINDOWS_JSON" "all windows"
run_step "focused window query" write_focused_window_json
run_step "validate focused limit" require_at_most_one_window "$FOCUSED_JSON" "focused query"

calculator_app="$(find_calculator_app)" \
  || skip_or_fail "GNOME Calculator is not installed; set PEEKABOOX_WINDOWS_CALCULATOR_APP to override"
run_step "launch Calculator" launch_calculator_app "$calculator_app"
sleep "$LAUNCH_DELAY"

run_step "query Calculator by app/title regex" write_calculator_window_json
run_step "validate Calculator window" require_window_result "$CALCULATOR_JSON" "calculator"

if calculator_id="$(extract_first_window_id "$CALCULATOR_JSON")"; then
  printf '%s\n' "$calculator_id" >"$CALCULATOR_ID_TXT"
  echo "Calculator window id: $calculator_id"
  run_step "query Calculator by id" write_calculator_by_id_json "$calculator_id"
  run_step "validate id query" require_same_window_id "$CALCULATOR_BY_ID_JSON" "$calculator_id"
  run_step "capture Calculator by window id" capture_calculator_window "$calculator_id"
  run_step "scope elements to Calculator window id" write_calculator_elements_json "$calculator_id"
else
  run_step "extract Calculator window id" false
fi

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX windows inventory example passed."
fi
