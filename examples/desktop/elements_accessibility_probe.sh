#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/elements-probe}}"
STRICT="${PEEKABOOX_STRICT:-0}"
TITLE="${PEEKABOOX_ELEMENTS_PROBE_TITLE:-PeekabooX Elements Example}"
APP_SCRIPT="$OUT_DIR/elements_probe_app.py"
BUTTON_JSON="$OUT_DIR/button-elements.json"
EXACT_JSON="$OUT_DIR/exact-regex-elements.json"
CONTAINS_JSON="$OUT_DIR/contains-elements.json"
VISION_JSON="$OUT_DIR/vision-fallback-elements.json"
app_pid=""
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

cleanup() {
  if [[ -n "$app_pid" ]] && kill -0 "$app_pid" >/dev/null 2>&1; then
    kill "$app_pid" >/dev/null 2>&1 || true
    wait "$app_pid" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

require_python_gtk() {
  python3 - <<'PY'
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk  # noqa: F401
PY
}

write_probe_app() {
  mkdir -p "$OUT_DIR"
  cat >"$APP_SCRIPT" <<'PY'
import os

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import Gtk


title = os.environ.get("PEEKABOOX_ELEMENTS_PROBE_TITLE", "PeekabooX Elements Example")

window = Gtk.Window(title=title)
window.set_default_size(520, 260)
window.connect("destroy", Gtk.main_quit)

grid = Gtk.Grid(column_spacing=12, row_spacing=12, margin=24)
window.add(grid)

heading = Gtk.Label(label="PeekabooX Elements Probe")
heading.set_xalign(0)
grid.attach(heading, 0, 0, 2, 1)

entry = Gtk.Entry()
entry.set_text("Editable target")
entry.set_tooltip_text("Editable target")
grid.attach(entry, 0, 1, 2, 1)

checkbox = Gtk.CheckButton(label="Enable semantic probe")
checkbox.set_active(True)
grid.attach(checkbox, 0, 2, 2, 1)

button = Gtk.Button(label="Run Elements Check")
grid.attach(button, 0, 3, 1, 1)

status = Gtk.Label(label="Status: waiting for PeekabooX")
status.set_xalign(0)
grid.attach(status, 0, 4, 2, 1)

window.show_all()
Gtk.main()
PY
}

extract_center_point() {
  python3 - "$BUTTON_JSON" <<'PY'
import json
import sys

payload = json.loads(open(sys.argv[1], encoding="utf-8").read())
elements = payload.get("elements", [])
if not elements:
    raise SystemExit("no elements in JSON output")
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

write_button_json() {
  run_peekaboox elements \
    --window-title "$TITLE" \
    --role "push button" \
    --text "Run Elements Check" \
    --json >"$BUTTON_JSON"
}

write_exact_json() {
  run_peekaboox elements \
    --window-title "$TITLE" \
    --selector "role-exact=push button,label-regex=^Run Elements,not-state=disabled,min-width=20,min-height=10" \
    --json >"$EXACT_JSON"
}

write_contains_json() {
  local point="$1"
  run_peekaboox elements \
    --window-title "$TITLE" \
    --contains "$point" \
    --json >"$CONTAINS_JSON"
}

write_vision_json() {
  run_peekaboox elements \
    --window-title "$TITLE" \
    --selector "role=visual-region,min-width=20" \
    --vision-fallback \
    --vision-threshold 24 \
    --vision-max-elements 50 \
    --json >"$VISION_JSON"
}

mkdir -p "$OUT_DIR"

if ! require_python_gtk; then
  echo "warning: python3 gi Gtk 3 bindings are required for this example" >&2
  [[ "$STRICT" == "1" ]] && exit 1
  exit 0
fi

write_probe_app
PEEKABOOX_ELEMENTS_PROBE_TITLE="$TITLE" python3 "$APP_SCRIPT" &
app_pid="$!"
sleep "${PEEKABOOX_ELEMENTS_PROBE_WAIT:-2}"

run_step "semantic button lookup scoped to window title" write_button_json

run_step "exact role, regex label, negative state, and size selector" write_exact_json

if [[ -s "$BUTTON_JSON" ]]; then
  if point="$(extract_center_point)"; then
    run_step "contains selector from returned center point" write_contains_json "$point"
  else
    failures=$((failures + 1))
    echo "warning: could not extract center point from $BUTTON_JSON" >&2
    [[ "$STRICT" == "1" ]] && exit 1
  fi
fi

run_step "scoped vision fallback with configurable detector options" write_vision_json

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX elements accessibility probe example passed."
fi
