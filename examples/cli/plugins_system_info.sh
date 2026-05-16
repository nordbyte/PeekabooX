#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${PEEKABOOX_EXAMPLE_OUT:-$ROOT/target/examples/plugins-system-info}"
RUN_ID="${PEEKABOOX_PLUGINS_RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
PLUGIN_ROOT="$ROOT/examples/plugins"
PLUGIN_MANIFEST="$ROOT/examples/plugins/system-info/peekaboox.plugin.json"
PLUGIN_ID="org.peekaboox.examples.system-info"
PLUGIN_TOOL="system_info.uname"
DISCOVERY_JSON="$OUT_DIR/plugins-discovery-$RUN_ID.json"
MANIFEST_DISCOVERY_JSON="$OUT_DIR/plugins-manifest-discovery-$RUN_ID.json"
CALL_JSON="$OUT_DIR/plugin-call-$RUN_ID.json"

run_peekaboox() {
  if [[ -n "${PEEKABOOX_BIN:-}" ]]; then
    "$PEEKABOOX_BIN" "$@"
  elif command -v peekaboox >/dev/null 2>&1; then
    peekaboox "$@"
  else
    (cd "$ROOT" && cargo run --quiet -p peekaboox-cli -- "$@")
  fi
}

validate_discovery() {
  local file="$1"
  python3 - "$file" "$PLUGIN_ID" "$PLUGIN_TOOL" <<'PY'
import json
import sys

path, plugin_id, plugin_tool = sys.argv[1:4]
payload = json.loads(open(path, encoding="utf-8").read())
if payload.get("sdk_version") != "peekaboox.plugin.v1":
    raise SystemExit(f"unexpected sdk_version: {payload.get('sdk_version')!r}")
if payload.get("errors"):
    raise SystemExit(f"unexpected plugin discovery errors: {payload['errors']!r}")
plugins = payload.get("plugins", [])
plugin = next((item for item in plugins if item.get("id") == plugin_id), None)
if plugin is None:
    raise SystemExit(f"missing plugin: {plugin_id}")
tools = {tool.get("name"): tool for tool in plugin.get("tools", [])}
tool = tools.get(plugin_tool)
if tool is None:
    raise SystemExit(f"missing plugin tool: {plugin_tool}")
schema = json.loads(tool.get("input_schema_json", "{}"))
if schema.get("additionalProperties") is not False:
    raise SystemExit("system_info.uname schema should reject additional properties")
print(json.dumps({"plugin": plugin_id, "tool": plugin_tool, "schema": "checked"}, sort_keys=True))
PY
}

validate_call() {
  local file="$1"
  python3 - "$file" "$PLUGIN_ID" "$PLUGIN_TOOL" <<'PY'
import json
import sys

path, plugin_id, plugin_tool = sys.argv[1:4]
payload = json.loads(open(path, encoding="utf-8").read())
if not payload.get("ok"):
    raise SystemExit(json.dumps(payload, indent=2))
if payload.get("plugin_id") != plugin_id:
    raise SystemExit(f"unexpected plugin_id: {payload.get('plugin_id')!r}")
if payload.get("tool") != plugin_tool:
    raise SystemExit(f"unexpected tool: {payload.get('tool')!r}")
result = payload.get("result")
if not isinstance(result, dict):
    raise SystemExit("plugin result must be an object")
required = {"system", "node", "release", "version", "machine", "processor"}
missing = sorted(required - set(result))
if missing:
    raise SystemExit(f"plugin result missing keys: {missing}")
print(json.dumps({"plugin": plugin_id, "result_keys": sorted(result)}, sort_keys=True))
PY
}

mkdir -p "$OUT_DIR"
for path in "$DISCOVERY_JSON" "$MANIFEST_DISCOVERY_JSON" "$CALL_JSON"; do
  if [[ -e "$path" ]]; then
    echo "error: refusing to overwrite existing file: $path" >&2
    exit 1
  fi
done

echo "== discover plugins from directory =="
run_peekaboox plugins --path "$PLUGIN_ROOT" --json >"$DISCOVERY_JSON"
cat "$DISCOVERY_JSON"
validate_discovery "$DISCOVERY_JSON"

echo
echo "== discover plugin from manifest path =="
run_peekaboox plugins --path "$PLUGIN_MANIFEST" --json >"$MANIFEST_DISCOVERY_JSON"
cat "$MANIFEST_DISCOVERY_JSON"
validate_discovery "$MANIFEST_DISCOVERY_JSON"

echo
echo "== call system_info.uname plugin tool =="
run_peekaboox plugin-call \
  "$PLUGIN_ID" \
  "$PLUGIN_TOOL" \
  --path "$PLUGIN_ROOT" \
  --arguments-json '{}' \
  --timeout-ms "${PEEKABOOX_PLUGIN_TIMEOUT_MS:-5000}" \
  --max-output-bytes "${PEEKABOOX_PLUGIN_MAX_OUTPUT_BYTES:-65536}" \
  --json >"$CALL_JSON"
cat "$CALL_JSON"
validate_call "$CALL_JSON"

echo "PeekabooX Plugin SDK CLI example passed."
