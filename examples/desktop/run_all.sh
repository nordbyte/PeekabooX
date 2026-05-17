#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DESKTOP_DIR="$ROOT/examples/desktop"
OUT_DIR="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/desktop-harness}"
STRICT="${PEEKABOOX_STRICT:-0}"
MODE="syntax"
INCLUDE_DESTRUCTIVE=0
CLOSE_WINDOWS=0
FILTERS=()

SAFE_EXAMPLES=(
  "desktop_profiles_registry.sh"
  "desktop_profiles_daemon_parity.sh"
)

LIVE_EXAMPLES=(
  "live_smoke.sh"
  "capture_backends_diagnostics.sh"
  "capture_window_targets.sh"
  "capture_daemon_mcp_targets.sh"
  "capture_delta_stream.sh"
  "windows_inventory.sh"
  "windows_live_smoke.sh"
  "elements_accessibility_probe.sh"
  "elements_calculator.sh"
  "ocr_visible_window.sh"
  "move_pointer_path.sh"
  "click_calculator_keypad.sh"
  "type_text_editor_input.sh"
  "paste_text_editor_input.sh"
  "hotkey_text_editor_save.sh"
  "paint_draw_and_save.sh"
  "text_editor_save_dialog.sh"
  "drag_absolute_canvas.sh"
)

DESTRUCTIVE_EXAMPLES=(
  "telegram_saved_messages.sh"
)

usage() {
  cat <<'EOF'
Usage: examples/desktop/run_all.sh [--syntax-only|--smoke|--live] [options]

Options:
  --list                   Print known desktop examples and exit.
  --syntax-only            Run bash syntax checks for all desktop shell examples.
  --smoke                  Run non-GUI registry/daemon smoke examples.
  --live                   Run live desktop examples. Telegram is skipped by default.
  --include-destructive    Include examples that send external messages.
  --filter <text>          Only include scripts whose filename contains <text>.
  --out-dir <path>         Write per-example output under <path>.
  --strict                 Treat warnings as failures where examples support it.
  --close                  Let examples close apps they launched when supported.
  --no-close               Keep app windows open when examples support it.
  -h, --help               Show this help.
EOF
}

all_shell_examples() {
  local script
  for script in "$DESKTOP_DIR"/*.sh; do
    basename "$script"
  done | sort
}

known_examples() {
  printf '%s\n' "${SAFE_EXAMPLES[@]}" "${LIVE_EXAMPLES[@]}" "${DESTRUCTIVE_EXAMPLES[@]}" | sort -u
}

matches_filters() {
  local script="$1"
  local filter
  if [[ "${#FILTERS[@]}" -eq 0 ]]; then
    return 0
  fi
  for filter in "${FILTERS[@]}"; do
    if [[ "$script" == *"$filter"* ]]; then
      return 0
    fi
  done
  return 1
}

selected_runtime_examples() {
  case "$MODE" in
    smoke)
      printf '%s\n' "${SAFE_EXAMPLES[@]}"
      ;;
    live)
      printf '%s\n' "${SAFE_EXAMPLES[@]}" "${LIVE_EXAMPLES[@]}"
      if [[ "$INCLUDE_DESTRUCTIVE" == "1" ]]; then
        printf '%s\n' "${DESTRUCTIVE_EXAMPLES[@]}"
      fi
      ;;
    syntax)
      return 0
      ;;
    *)
      echo "unknown mode: $MODE" >&2
      return 2
      ;;
  esac | sort -u
}

run_syntax_checks() {
  local failures=0
  local script
  while IFS= read -r script; do
    matches_filters "$script" || continue
    printf 'syntax: %s\n' "$script"
    if ! bash -n "$DESKTOP_DIR/$script"; then
      failures=$((failures + 1))
    fi
  done < <(all_shell_examples)
  return "$failures"
}

export_close_policy() {
  local value="$1"
  export PEEKABOOX_CAPTURE_CLOSE_APP="$value"
  export PEEKABOOX_CAPTURE_PARITY_CLOSE_APP="$value"
  export PEEKABOOX_ELEMENTS_CALCULATOR_CLOSE="$value"
  export PEEKABOOX_PAINT_CLOSE="$value"
  export PEEKABOOX_PASTE_CLOSE="$value"
  export PEEKABOOX_TEXT_EDITOR_CLOSE="$value"
  export PEEKABOOX_TYPE_CLOSE="$value"
  export PEEKABOOX_WINDOWS_CLOSE="$value"
}

run_runtime_examples() {
  local failures=0
  local script
  mkdir -p "$OUT_DIR"
  export PEEKABOOX_STRICT="$STRICT"
  export_close_policy "$CLOSE_WINDOWS"

  while IFS= read -r script; do
    [[ -n "$script" ]] || continue
    matches_filters "$script" || continue
    local script_out="$OUT_DIR/${script%.sh}"
    mkdir -p "$script_out"
    printf '\n== %s ==\n' "$script"
    if ! "$DESKTOP_DIR/$script" "$script_out"; then
      failures=$((failures + 1))
      echo "failed: $script" >&2
      if [[ "$STRICT" == "1" ]]; then
        return "$failures"
      fi
    fi
  done < <(selected_runtime_examples)

  return "$failures"
}

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --list)
      known_examples
      exit 0
      ;;
    --syntax-only)
      MODE="syntax"
      shift
      ;;
    --smoke)
      MODE="smoke"
      shift
      ;;
    --live)
      MODE="live"
      shift
      ;;
    --include-destructive)
      INCLUDE_DESTRUCTIVE=1
      shift
      ;;
    --filter)
      if [[ "$#" -lt 2 ]]; then
        echo "missing value for --filter" >&2
        exit 2
      fi
      FILTERS+=("$2")
      shift 2
      ;;
    --out-dir)
      if [[ "$#" -lt 2 ]]; then
        echo "missing value for --out-dir" >&2
        exit 2
      fi
      OUT_DIR="$2"
      shift 2
      ;;
    --strict)
      STRICT=1
      shift
      ;;
    --close)
      CLOSE_WINDOWS=1
      shift
      ;;
    --no-close)
      CLOSE_WINDOWS=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

syntax_failures=0
runtime_failures=0

run_syntax_checks || syntax_failures="$?"

if [[ "$MODE" != "syntax" ]]; then
  run_runtime_examples || runtime_failures="$?"
fi

if [[ "$syntax_failures" -ne 0 || "$runtime_failures" -ne 0 ]]; then
  echo
  echo "Desktop example harness failed: syntax=$syntax_failures runtime=$runtime_failures" >&2
  exit 1
fi

echo
case "$MODE" in
  syntax)
    echo "Desktop example syntax checks passed."
    ;;
  smoke)
    echo "Desktop example smoke checks passed. Output: $OUT_DIR"
    ;;
  live)
    echo "Desktop example live checks passed. Output: $OUT_DIR"
    ;;
esac
