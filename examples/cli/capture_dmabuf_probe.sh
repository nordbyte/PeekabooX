#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/capture-dmabuf}"
RUN_ID="${PEEKABOOX_DMABUF_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
BACKENDS_JSON="$OUT_DIR/capture-backends-dmabuf-$RUN_ID.json"
HELP_TXT="$OUT_DIR/capture-dmabuf-help-$RUN_ID.txt"
LIVE_TXT="$OUT_DIR/capture-dmabuf-live-$RUN_ID.txt"
STRICT="${PEEKABOOX_STRICT:-0}"
LIVE="${PEEKABOOX_DMABUF_LIVE:-0}"
IMPORT_TARGET="${PEEKABOOX_DMABUF_IMPORT:-compute}"

run_peekaboox() {
  if [[ -n "${PEEKABOOX_BIN:-}" ]]; then
    "$PEEKABOOX_BIN" "$@"
  elif command -v peekaboox >/dev/null 2>&1; then
    peekaboox "$@"
  else
    (cd "$ROOT" && cargo run --quiet -p peekaboox-cli -- "$@")
  fi
}

validate_backends_json() {
  python3 - "$BACKENDS_JSON" <<'PY'
import json
import sys

path = sys.argv[1]
payload = json.loads(open(path, encoding="utf-8").read())
required = {
    "session_type",
    "pipewire_session_available",
    "pipewire_backend_feature_enabled",
    "egl_backend_feature_enabled",
    "zero_copy_backends",
}
missing = sorted(required - set(payload))
if missing:
    raise SystemExit(f"capture-backends output missing keys: {missing}")
zero_copy = payload.get("zero_copy_backends")
if not isinstance(zero_copy, list):
    raise SystemExit("zero_copy_backends must be a list")
dmabuf = [
    item for item in zero_copy
    if "dmabuf" in str(item.get("transport", "")).lower()
    or "dmabuf" in str(item.get("name", "")).lower()
]
summary = {
    "session_type": payload.get("session_type"),
    "pipewire_session_available": payload.get("pipewire_session_available"),
    "pipewire_backend_feature_enabled": payload.get("pipewire_backend_feature_enabled"),
    "egl_backend_feature_enabled": payload.get("egl_backend_feature_enabled"),
    "dmabuf_zero_copy_backends": len(dmabuf),
    "warnings": payload.get("warnings", []),
}
print(json.dumps(summary, sort_keys=True))
PY
}

validate_live_output() {
  python3 - "$LIVE_TXT" "$IMPORT_TARGET" <<'PY'
import sys

path, import_target = sys.argv[1:3]
text = open(path, encoding="utf-8").read()
for token in ("dmabuf_stream", "dmabuf_frame", "dmabuf_import"):
    if token not in text:
        raise SystemExit(f"live output missing {token!r}")
expected = import_target.replace("_", "-")
if f"target={expected}" not in text:
    raise SystemExit(f"live output missing target={expected}")
PY
}

mkdir -p "$OUT_DIR"
for path in "$BACKENDS_JSON" "$HELP_TXT" "$LIVE_TXT"; do
  if [[ -e "$path" ]]; then
    echo "error: refusing to overwrite existing file: $path" >&2
    exit 1
  fi
done

echo "== capture-dmabuf help =="
run_peekaboox capture-dmabuf --help >"$HELP_TXT"
cat "$HELP_TXT"
grep -q -- "--import <compute|egl|egl-texture>" "$HELP_TXT"

echo
echo "== DMA-BUF backend diagnostics =="
run_peekaboox capture-backends --diagnose --json --probe none >"$BACKENDS_JSON"
cat "$BACKENDS_JSON"
validate_backends_json

if [[ "$LIVE" != "1" ]]; then
  echo
  echo "Diagnostics completed. Set PEEKABOOX_DMABUF_LIVE=1 to run the live capture-dmabuf probe."
  exit 0
fi

echo
echo "== live capture-dmabuf probe =="
if run_peekaboox capture-dmabuf --import "$IMPORT_TARGET" >"$LIVE_TXT" 2>&1; then
  cat "$LIVE_TXT"
  validate_live_output
  echo "PeekabooX capture-dmabuf CLI example passed."
  exit 0
fi

cat "$LIVE_TXT" >&2
echo "warning: live capture-dmabuf probe failed; this requires a PipeWire/Portal session and a feature-enabled build" >&2
if [[ "$STRICT" == "1" ]]; then
  exit 1
fi
exit 0
