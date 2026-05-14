#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/live-smoke}}"
STRICT="${PEEKABOOX_STRICT:-0}"
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

mkdir -p "$OUT_DIR"

echo "PeekabooX live desktop smoke output: $OUT_DIR"
run_step "version" run_peekaboox --version
run_step "capture backend discovery" run_peekaboox capture-backends
run_step "screen capture" run_peekaboox capture --output "$OUT_DIR/screenshot.png"
run_step "window enumeration" run_peekaboox windows
run_step "semantic element scan" run_peekaboox elements --limit 10
run_step "dry-run click at 10,10" run_peekaboox click --x 10 --y 10 --dry-run
run_step "dry-run pointer move" run_peekaboox move --x 10 --y 10 --dry-run
run_step "dry-run pointer drag" run_peekaboox drag --from 10,10 --to 30,30 --dry-run
run_step "dry-run text input" run_peekaboox type --dry-run "PeekabooX live smoke"
run_step "dry-run hotkey" run_peekaboox hotkey --dry-run ctrl+s

if [[ -f "$OUT_DIR/screenshot.png" ]]; then
  run_step "self-compare captured screenshot" \
    run_peekaboox compare "$OUT_DIR/screenshot.png" "$OUT_DIR/screenshot.png"
fi

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX live desktop smoke example passed."
fi
