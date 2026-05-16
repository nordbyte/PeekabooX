#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${1:-${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/desktop-profiles}}"
RUN_ID="${PEEKABOOX_DESKTOP_PROFILES_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
ALL_JSON="$OUT_DIR/profiles-all-$RUN_ID.json"
TELEGRAM_JSON="$OUT_DIR/profiles-telegram-type-into-$RUN_ID.json"
AVAILABILITY_JSON="$OUT_DIR/profiles-availability-$RUN_ID.json"

run_peekaboox() {
  if [[ -n "${PEEKABOOX_BIN:-}" ]]; then
    "$PEEKABOOX_BIN" "$@"
  elif command -v peekaboox >/dev/null 2>&1; then
    peekaboox "$@"
  else
    cargo run --quiet -p peekaboox-cli -- "$@"
  fi
}

mkdir -p "$OUT_DIR"

run_peekaboox desktop profiles --json >"$ALL_JSON"
run_peekaboox desktop profiles \
  --app telegram \
  --supports type-into \
  --target message-input \
  --json >"$TELEGRAM_JSON"
run_peekaboox desktop profiles \
  --availability \
  --command flatpak \
  --json >"$AVAILABILITY_JSON"

python3 - "$ALL_JSON" "$TELEGRAM_JSON" "$AVAILABILITY_JSON" <<'PY'
import json
import sys

all_path, telegram_path, availability_path = sys.argv[1:4]
all_profiles = json.loads(open(all_path, encoding="utf-8").read())
telegram = json.loads(open(telegram_path, encoding="utf-8").read())
availability = json.loads(open(availability_path, encoding="utf-8").read())

if all_profiles.get("schema_version") != "desktop-profiles.v1":
    raise SystemExit("missing desktop profile schema_version")
if all_profiles.get("count", 0) < 1:
    raise SystemExit("no desktop profiles returned")

telegram_profiles = telegram.get("profiles", [])
if len(telegram_profiles) != 1 or telegram_profiles[0].get("id") != "telegram":
    raise SystemExit("telegram profile filter did not return exactly telegram")

targets = telegram_profiles[0].get("targets", [])
message_input = next((target for target in targets if target.get("name") == "message-input"), None)
if not message_input:
    raise SystemExit("message-input target missing")
if "type-into" not in message_input.get("supports", []):
    raise SystemExit("message-input does not advertise type-into support")

commands = telegram_profiles[0].get("commands", [])
if not any(command.get("display") == "flatpak run org.telegram.desktop" for command in commands):
    raise SystemExit("flatpak command arguments were not preserved")

availability_profiles = availability.get("profiles", [])
if not availability_profiles:
    raise SystemExit("availability filter returned no profiles")
if not all(profile.get("availability", {}).get("checked") for profile in availability_profiles):
    raise SystemExit("availability check flag was not reflected in JSON")

print(f"profiles: {all_profiles['count']}")
print(f"telegram targets: {', '.join(target['name'] for target in targets)}")
print(f"availability profiles: {', '.join(profile['id'] for profile in availability_profiles)}")
PY

printf '\nWrote:\n'
printf '  %s\n' "$ALL_JSON"
printf '  %s\n' "$TELEGRAM_JSON"
printf '  %s\n' "$AVAILABILITY_JSON"
