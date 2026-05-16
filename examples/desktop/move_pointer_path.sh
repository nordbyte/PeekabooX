#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/move-pointer-path}}"
RUN_ID="${PEEKABOOX_MOVE_POINTER_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
LIVE="${PEEKABOOX_MOVE_POINTER_LIVE:-0}"
STRICT="${PEEKABOOX_STRICT:-0}"
BACKEND="${PEEKABOOX_MOVE_POINTER_BACKEND:-auto}"
POSITION_JSON="$OUT_DIR/cursor-position-$RUN_ID.json"
DRY_RUN_JSON="$OUT_DIR/move-dry-run-$RUN_ID.json"
REGION_JSON="$OUT_DIR/move-region-ratio-$RUN_ID.json"
RELATIVE_JSON="$OUT_DIR/move-relative-$RUN_ID.json"
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

json_cursor_point() {
  python3 - "$1" <<'PY'
import json
import sys

payload = json.loads(open(sys.argv[1], encoding="utf-8").read())
cursor = payload["cursor"]
print(f'{int(cursor["x"])},{int(cursor["y"])}')
PY
}

mkdir -p "$OUT_DIR"
echo "PeekabooX move pointer path output: $OUT_DIR"

run_step_to_file "query current cursor position" "$POSITION_JSON" \
  run_peekaboox move --current-position --json

run_step_to_file "dry-run compact target with smooth movement options" "$DRY_RUN_JSON" \
  run_peekaboox move --to 64,64 --duration-ms 160 --steps 8 --backend "$BACKEND" --clamp --dry-run --json

run_step_to_file "dry-run region-relative ratio target" "$REGION_JSON" \
  run_peekaboox move --region 0,0,240,160 --ratio 0.75,0.25 --bounds clamp --dry-run --json

run_step_to_file "dry-run relative movement" "$RELATIVE_JSON" \
  run_peekaboox move --relative 16,12 --duration-ms 80 --steps 4 --backend "$BACKEND" --dry-run --json

if [[ "$LIVE" == "1" ]]; then
  if [[ ! -s "$POSITION_JSON" ]]; then
    run_step "live path skipped because cursor position was unavailable" false
  else
    original="$(json_cursor_point "$POSITION_JSON")" || original=""
    if [[ -z "$original" ]]; then
      run_step "live path skipped because cursor JSON could not be parsed" false
    else
      run_step "live smooth pointer path" \
        run_peekaboox move --to 80,80 --duration-ms 120 --steps 6 --backend "$BACKEND" --clamp
      run_step "live relative pointer segment" \
        run_peekaboox move --relative 120,0 --duration-ms 120 --steps 6 --backend "$BACKEND" --clamp
      run_step "live scoped ratio pointer segment" \
        run_peekaboox move --region 0,0,260,180 --ratio 0.85,0.75 --duration-ms 120 --steps 6 --backend "$BACKEND" --clamp
      run_step "restore original cursor position" \
        run_peekaboox move --to "$original" --duration-ms 120 --steps 6 --backend "$BACKEND" --clamp
    fi
  fi
else
  echo
  echo "Dry-run mode only. Set PEEKABOOX_MOVE_POINTER_LIVE=1 for live pointer movement."
fi

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX move pointer path example passed."
fi
