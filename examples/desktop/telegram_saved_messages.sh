#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/telegram-saved-messages}}"
TELEGRAM_LOG="$OUT_DIR/telegram-app.log"
WINDOWS_FILE="$OUT_DIR/windows.txt"
AFTER_LAUNCH_CAPTURE="$OUT_DIR/after-launch.png"
AFTER_SEARCH_CAPTURE="$OUT_DIR/after-search.png"
AFTER_SEND_CAPTURE="$OUT_DIR/after-send.png"
STRICT="${PEEKABOOX_STRICT:-0}"
MESSAGE="${PEEKABOOX_TELEGRAM_MESSAGE:-PeekabooX Example}"
SEARCH_QUERY="${PEEKABOOX_TELEGRAM_SEARCH_QUERY:-Saved Messages}"
LAUNCH_DELAY="${PEEKABOOX_TELEGRAM_LAUNCH_DELAY:-5}"
SEARCH_DELAY="${PEEKABOOX_TELEGRAM_SEARCH_DELAY:-1}"
OPEN_DELAY="${PEEKABOOX_TELEGRAM_OPEN_DELAY:-1}"
SEND_DELAY="${PEEKABOOX_TELEGRAM_SEND_DELAY:-1}"
FOCUS_DELAY="${PEEKABOOX_TELEGRAM_FOCUS_DELAY:-0.6}"
FOCUS_APP_DELAY="${PEEKABOOX_TELEGRAM_FOCUS_APP_DELAY:-1}"
TYPE_DELAY="${PEEKABOOX_TELEGRAM_TYPE_DELAY:-0.8}"
REQUIRE_FOCUS="${PEEKABOOX_TELEGRAM_REQUIRE_FOCUS:-1}"
WINDOW_GUARD="${PEEKABOOX_TELEGRAM_WINDOW_GUARD:-1}"
telegram_pid=""
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

warn_or_exit() {
  local message="$1"
  failures=$((failures + 1))
  echo "warning: $message" >&2
  if [[ "$STRICT" == "1" ]]; then
    exit 1
  fi
  exit 0
}

find_telegram_app() {
  if [[ -n "${PEEKABOOX_TELEGRAM_APP:-}" ]]; then
    printf '%s\n' "$PEEKABOOX_TELEGRAM_APP"
    return 0
  fi

  if command -v telegram-desktop >/dev/null 2>&1; then
    printf '%s\n' "telegram-desktop"
    return 0
  fi

  if command -v telegram >/dev/null 2>&1; then
    printf '%s\n' "telegram"
    return 0
  fi

  if command -v flatpak >/dev/null 2>&1 && flatpak info org.telegram.desktop >/dev/null 2>&1; then
    printf '%s\n' "flatpak run org.telegram.desktop"
    return 0
  fi

  return 1
}

launch_telegram_app() {
  local app_command="$1"
  local app_argv=()
  read -r -a app_argv <<<"$app_command"
  "${app_argv[@]}" >"$TELEGRAM_LOG" 2>&1 &
  telegram_pid="$!"
}

telegram_running() {
  pgrep -af 'telegram-desktop|Telegram|org.telegram.desktop' 2>/dev/null \
    | grep -v -E 'pgrep -af|grep -v|telegram_saved_messages' >/dev/null
}

capture_windows() {
  run_peekaboox windows | tee "$WINDOWS_FILE"
}

telegram_window_visible() {
  grep -Eiq 'Telegram|telegram-desktop|org\.telegram\.desktop' "$WINDOWS_FILE"
}

telegram_window_focused() {
  grep -Eiq 'yes[[:space:]].*(Telegram|telegram-desktop|org\.telegram\.desktop)' "$WINDOWS_FILE"
}

ensure_telegram_app_started() {
  if [[ -n "$telegram_pid" ]] && kill -0 "$telegram_pid" >/dev/null 2>&1; then
    return 0
  fi

  if telegram_running; then
    return 0
  fi

  echo "warning: Telegram exited before input automation could start" >&2
  if [[ -s "$TELEGRAM_LOG" ]]; then
    sed -n '1,30p' "$TELEGRAM_LOG" >&2
  fi
  warn_or_exit "Telegram did not stay running"
}

ensure_telegram_window_ready() {
  run_step "window enumeration after Telegram launch" capture_windows

  if ! telegram_window_visible; then
    if [[ "$WINDOW_GUARD" == "0" ]]; then
      echo "warning: no Telegram window was detected; continuing because PEEKABOOX_TELEGRAM_WINDOW_GUARD=0" >&2
      return 0
    fi
    warn_or_exit "no Telegram window was detected; log written to $TELEGRAM_LOG"
  fi

  if [[ "$REQUIRE_FOCUS" == "1" ]] && ! telegram_window_focused; then
    if [[ "$WINDOW_GUARD" == "0" ]]; then
      echo "warning: Telegram focus was not confirmed; continuing because PEEKABOOX_TELEGRAM_WINDOW_GUARD=0" >&2
      return 0
    fi
    warn_or_exit "Telegram window is visible but not focused; set PEEKABOOX_TELEGRAM_REQUIRE_FOCUS=0 to bypass this guard"
  fi
}

focus_telegram_window() {
  if [[ -n "${PEEKABOOX_TELEGRAM_FOCUS_X:-}" && -n "${PEEKABOOX_TELEGRAM_FOCUS_Y:-}" ]]; then
    run_peekaboox click --x "$PEEKABOOX_TELEGRAM_FOCUS_X" --y "$PEEKABOOX_TELEGRAM_FOCUS_Y"
  fi
}

focus_search_bar() {
  if [[ -n "${PEEKABOOX_TELEGRAM_SEARCH_X:-}" && -n "${PEEKABOOX_TELEGRAM_SEARCH_Y:-}" ]]; then
    run_peekaboox click --x "$PEEKABOOX_TELEGRAM_SEARCH_X" --y "$PEEKABOOX_TELEGRAM_SEARCH_Y"
    return
  fi

  run_peekaboox hotkey ctrl+k
}

clear_search_bar() {
  if [[ -n "${PEEKABOOX_TELEGRAM_CLEAR_X:-}" && -n "${PEEKABOOX_TELEGRAM_CLEAR_Y:-}" ]]; then
    run_peekaboox click --x "$PEEKABOOX_TELEGRAM_CLEAR_X" --y "$PEEKABOOX_TELEGRAM_CLEAR_Y"
    return
  fi

  run_peekaboox hotkey ctrl+a
}

open_search_result() {
  if [[ -n "${PEEKABOOX_TELEGRAM_RESULT_X:-}" && -n "${PEEKABOOX_TELEGRAM_RESULT_Y:-}" ]]; then
    run_peekaboox click --x "$PEEKABOOX_TELEGRAM_RESULT_X" --y "$PEEKABOOX_TELEGRAM_RESULT_Y"
    return
  fi

  run_peekaboox hotkey Enter
}

focus_message_input() {
  if [[ -n "${PEEKABOOX_TELEGRAM_INPUT_X:-}" && -n "${PEEKABOOX_TELEGRAM_INPUT_Y:-}" ]]; then
    run_peekaboox click --x "$PEEKABOOX_TELEGRAM_INPUT_X" --y "$PEEKABOOX_TELEGRAM_INPUT_Y"
  fi
}

send_message() {
  if [[ -n "${PEEKABOOX_TELEGRAM_SEND_X:-}" && -n "${PEEKABOOX_TELEGRAM_SEND_Y:-}" ]]; then
    run_peekaboox click --x "$PEEKABOOX_TELEGRAM_SEND_X" --y "$PEEKABOOX_TELEGRAM_SEND_Y"
    return
  fi

  run_peekaboox hotkey Enter
}

maybe_close_telegram() {
  if [[ "${PEEKABOOX_TELEGRAM_CLOSE:-0}" == "1" ]]; then
    run_step "close Telegram window" run_peekaboox hotkey Alt+F4
  fi
}

mkdir -p "$OUT_DIR"

echo "PeekabooX Telegram example output: $OUT_DIR"
echo "Search query: $SEARCH_QUERY"
echo "Message: $MESSAGE"

if ! telegram_app="$(find_telegram_app)"; then
  warn_or_exit "Telegram Desktop was not found; install telegram-desktop or set PEEKABOOX_TELEGRAM_APP"
fi

echo "Launching Telegram: $telegram_app"
launch_telegram_app "$telegram_app"
sleep "$LAUNCH_DELAY"
ensure_telegram_app_started
if [[ -n "${PEEKABOOX_TELEGRAM_FOCUS_X:-}" && -n "${PEEKABOOX_TELEGRAM_FOCUS_Y:-}" ]]; then
  run_step "focus Telegram window override" focus_telegram_window
  sleep "$FOCUS_APP_DELAY"
fi
ensure_telegram_window_ready

run_step "capture Telegram after launch" run_peekaboox capture --output "$AFTER_LAUNCH_CAPTURE"
run_step "focus Telegram search bar" focus_search_bar
sleep "$FOCUS_DELAY"
run_step "clear search bar" clear_search_bar
sleep "$TYPE_DELAY"
run_step "search Saved Messages chat" run_peekaboox type "$SEARCH_QUERY"
sleep "$SEARCH_DELAY"
run_step "capture Telegram search results" run_peekaboox capture --output "$AFTER_SEARCH_CAPTURE"
run_step "open Saved Messages search result" open_search_result
sleep "$OPEN_DELAY"
run_step "focus message input override" focus_message_input
sleep "$FOCUS_DELAY"
run_step "type test message" run_peekaboox type "$MESSAGE"
run_step "send test message" send_message
sleep "$SEND_DELAY"
run_step "capture Telegram after sending" run_peekaboox capture --output "$AFTER_SEND_CAPTURE"

maybe_close_telegram

if [[ "$failures" -gt 0 ]]; then
  echo
  echo "Completed with $failures warning(s). Set PEEKABOOX_STRICT=1 to treat them as failures."
else
  echo
  echo "PeekabooX Telegram Saved Messages example passed."
fi
