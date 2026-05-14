#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/telegram-saved-messages}}"
AFTER_FOCUS_CAPTURE="$OUT_DIR/after-focus.png"
AFTER_SEARCH_CAPTURE="$OUT_DIR/after-search.png"
AFTER_OPEN_CAPTURE="$OUT_DIR/after-open.png"
AFTER_SEND_CAPTURE="$OUT_DIR/after-send.png"
STRICT="${PEEKABOOX_STRICT:-0}"
MESSAGE="${PEEKABOOX_TELEGRAM_MESSAGE:-PeekabooX Example}"
SEARCH_QUERY="${PEEKABOOX_TELEGRAM_SEARCH_QUERY:-Saved Messages}"
FOCUS_WAIT_MS="${PEEKABOOX_TELEGRAM_FOCUS_WAIT_MS:-1000}"
OVERVIEW_WAIT_MS="${PEEKABOOX_TELEGRAM_OVERVIEW_WAIT_MS:-800}"
SEARCH_DELAY="${PEEKABOOX_TELEGRAM_SEARCH_DELAY:-1}"
OPEN_DELAY="${PEEKABOOX_TELEGRAM_OPEN_DELAY:-1}"
SEND_DELAY="${PEEKABOOX_TELEGRAM_SEND_DELAY:-1}"
ASSERT_HEADER="${PEEKABOOX_TELEGRAM_ASSERT_HEADER:-0}"
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

require_step() {
  local description="$1"
  shift
  printf '\n== %s ==\n' "$description"
  if "$@"; then
    return 0
  fi

  failures=$((failures + 1))
  echo "warning: $description failed" >&2
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
  exit 0
}

mkdir -p "$OUT_DIR"

echo "PeekabooX Telegram example output: $OUT_DIR"
echo "Search query: $SEARCH_QUERY"
echo "Message: $MESSAGE"

require_step "focus or launch Telegram" \
  run_peekaboox desktop focus --app telegram --wait-ms "$FOCUS_WAIT_MS" --overview-wait-ms "$OVERVIEW_WAIT_MS"
run_step "capture Telegram after focus" run_peekaboox capture --output "$AFTER_FOCUS_CAPTURE"

require_step "search Saved Messages chat" \
  run_peekaboox desktop type-into --app telegram --target search-input --clear "$SEARCH_QUERY"
sleep "$SEARCH_DELAY"
run_step "capture Telegram search results" run_peekaboox capture --output "$AFTER_SEARCH_CAPTURE"
require_step "verify search text did not land in message input" \
  run_peekaboox desktop assert --app telegram --target send-button --not-active

require_step "open Saved Messages search result" \
  run_peekaboox desktop click --app telegram --target search-result
sleep "$OPEN_DELAY"
run_step "capture Telegram after opening search result" run_peekaboox capture --output "$AFTER_OPEN_CAPTURE"

if [[ "$ASSERT_HEADER" == "1" ]]; then
  require_step "verify Saved Messages header with OCR/accessibility" \
    run_peekaboox desktop assert --app telegram --target header --contains "$SEARCH_QUERY"
fi

require_step "type test message" \
  run_peekaboox desktop type-into --app telegram --target message-input --clear "$MESSAGE"
require_step "verify message draft is ready" \
  run_peekaboox desktop assert --app telegram --target send-button --active
require_step "send test message" \
  run_peekaboox desktop click --app telegram --target send-button
sleep "$SEND_DELAY"
run_step "capture Telegram after sending" run_peekaboox capture --output "$AFTER_SEND_CAPTURE"

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX Telegram Saved Messages example passed."
fi
