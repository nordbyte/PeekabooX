#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FEATURES="${PEEKABOOX_INSTALL_FEATURES:-}"

install_args=(--locked)
if [[ -n "${FEATURES}" ]]; then
  install_args+=(--features "${FEATURES}")
fi

cargo install "${install_args[@]}" --path "${ROOT_DIR}/cli" --bin peekaboox
cargo install "${install_args[@]}" --path "${ROOT_DIR}/rust/daemon" --bin peekabooxd
