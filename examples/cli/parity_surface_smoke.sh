#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${ROOT_DIR}/target/examples/parity-surface"
BIN="${PEEKABOOX_BIN:-}"

mkdir -p "${OUT_DIR}"

if [[ -z "${BIN}" ]]; then
  BIN="cargo run -q -p peekaboox-cli --"
fi

export XDG_CONFIG_HOME="${OUT_DIR}/config"
export XDG_STATE_HOME="${OUT_DIR}/state"

run() {
  # shellcheck disable=SC2086
  ${BIN} "$@"
}

run tools --json > "${OUT_DIR}/tools.json"
run completions bash > "${OUT_DIR}/peekaboox.bash"
run config init
run config set capture.format '"jpeg"'
run config show > "${OUT_DIR}/config.json"
run permissions --json > "${OUT_DIR}/permissions.json"
run agent --goal "List the visible desktop state" --dry-run --json > "${OUT_DIR}/agent.json"
run agent list-sessions --json > "${OUT_DIR}/sessions.json"
run app list --json > "${OUT_DIR}/apps.json"
run clean --dry-run --json > "${OUT_DIR}/clean.json"

if command -v xdotool >/dev/null 2>&1 || command -v ydotool >/dev/null 2>&1; then
  run press enter --dry-run --json > "${OUT_DIR}/press.json" || true
  run scroll down --amount 1 --dry-run --json > "${OUT_DIR}/scroll.json" || true
fi

echo "wrote parity surface artifacts to ${OUT_DIR}"
