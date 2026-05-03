#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "v0.8.1 integration coverage validation currently requires Linux" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
DAEMON_PID=""
cleanup() {
  if [[ -n "$DAEMON_PID" ]]; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

pick_port() {
  python3 - <<'PY'
import socket
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}

DAEMON_PORT="$(pick_port)"
WORKSPACE_DIR="$TMP_DIR/workspace"
CONFIG_PATH="$TMP_DIR/config.yaml"
STORE_PATH="$TMP_DIR/store.jsonl"
TRACE_PATH="$TMP_DIR/trace.json"
GRAPH_PATH="$TMP_DIR/graph.yaml"

mkdir -p "$WORKSPACE_DIR"

cat >"$CONFIG_PATH" <<YAML
version: 1

daemon:
  node_id: local
  grpc_listen: 127.0.0.1:$DAEMON_PORT
  workspace: $WORKSPACE_DIR
  advertise_lan: false
  store: $STORE_PATH

client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:$DAEMON_PORT

policy:
  subject: integration-test
  fs:
    mounts:
      - name: workspace
        path: /
        permissions:
          read: true
          write: true
          delete: true
  exec:
    allowed_cwds:
      - /
    default_timeout_secs: 30
    max_timeout_secs: 60
    preserve_env: false
    env_allowlist: []
    allowed_secrets: []
  service:
    services:
      - id: local-daemon
        name: local-daemon
        host: 127.0.0.1
        port: $DAEMON_PORT
        protocol: tcp
        description: Operon daemon under integration test
        permissions:
          check: true
          forward: true
YAML

cargo build --workspace --locked
OPERON="$ROOT_DIR/target/debug/operon"
OPEROND="$ROOT_DIR/target/debug/operond"

"$OPEROND" start --config "$CONFIG_PATH" >"$TMP_DIR/operond.log" 2>&1 &
DAEMON_PID="$!"

for _ in $(seq 1 80); do
  if "$OPERON" --config "$CONFIG_PATH" node ping local >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

"$OPERON" --config "$CONFIG_PATH" --json config explain >"$TMP_DIR/config-explain.json"
python3 - "$TMP_DIR/config-explain.json" "$DAEMON_PORT" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    config = json.load(handle)
assert config["daemon"]["node_id"] == "local", config
assert config["client"]["nodes"][0]["endpoint"].endswith(":" + sys.argv[2]), config
assert config["policy"]["services"][0]["id"] == "local-daemon", config
PY

"$OPERON" --config "$CONFIG_PATH" node ping local | grep -q "ok=true"
"$OPERON" --config "$CONFIG_PATH" capability list local | grep -q "fs:workspace"
"$OPERON" --config "$CONFIG_PATH" service check local local-daemon | grep -q "ok=true"
"$OPERON" completion bash | grep -q "complete -F"
"$OPERON" completion zsh | grep -q "#compdef operon"

"$OPERON" --config "$CONFIG_PATH" fs write local:/hello.txt --content "hello integration"
"$OPERON" --config "$CONFIG_PATH" fs read local:/hello.txt >"$TMP_DIR/hello.out"
grep -q "hello integration" "$TMP_DIR/hello.out"
"$OPERON" --config "$CONFIG_PATH" fs copy local:/hello.txt local:/copy.txt
"$OPERON" --config "$CONFIG_PATH" fs truncate local:/copy.txt --size 5
"$OPERON" --config "$CONFIG_PATH" --json fs stat local:/copy.txt >"$TMP_DIR/copy-stat.json"
python3 - "$TMP_DIR/copy-stat.json" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    stat = json.load(handle)
assert stat["size"] == 5, stat
PY
"$OPERON" --config "$CONFIG_PATH" fs rm local:/copy.txt

"$OPERON" --config "$CONFIG_PATH" --json exec run local --timeout-secs 10 -- "printf exec-integration" \
  >"$TMP_DIR/exec.json"
EXEC_ID="$(
  python3 - "$TMP_DIR/exec.json" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    exec = json.load(handle)
assert exec["status"] == "succeeded", exec
assert exec["exit_code"] == 0, exec
print(exec["id"])
PY
)"
"$OPERON" --config "$CONFIG_PATH" exec logs local "$EXEC_ID" --stream >"$TMP_DIR/exec-logs.out"
grep -q "exec-integration" "$TMP_DIR/exec-logs.out"

cat >"$GRAPH_PATH" <<'YAML'
name: integration-graph
steps:
  - id: write-input
    node: local
    action: fs.write
    path: /graph-input.txt
    content: hello graph
  - id: run-exec
    node: local
    action: exec.run
    cwd: /
    timeout_secs: 10
    command: cat graph-input.txt > graph-output.txt
  - id: read-output
    node: local
    action: fs.read
    path: /graph-output.txt
YAML
"$OPERON" --config "$CONFIG_PATH" run "$GRAPH_PATH" --trace-output "$TRACE_PATH" \
  >"$TMP_DIR/graph.out"
grep -q "integration-graph" "$TMP_DIR/graph.out"
"$OPERON" trace show "$TRACE_PATH" >"$TMP_DIR/trace.out"
grep -q "Succeeded" "$TMP_DIR/trace.out"

"$OPERON" --config "$CONFIG_PATH" --json audit show local \
  --capability fs:workspace \
  --action write-stream \
  --allowed true \
  --limit 1 \
  >"$TMP_DIR/audit-write.json"
python3 - "$TMP_DIR/audit-write.json" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    audit = json.load(handle)
events = audit["events"]
assert len(events) == 1, events
assert events[0]["capability"] == "fs:workspace", events
assert events[0]["action"] == "write-stream", events
assert events[0]["allowed"] is True, events
PY

cargo test --workspace --locked -- --list >"$TMP_DIR/test-list.txt"
for expected in \
  "loads_unified_config_with_client_nodes" \
  "policy_config_round_trips_from_yaml" \
  "filesystem_capability_id_is_stable" \
  "mount_capability_constant_is_exported_at_crate_root" \
  "service_removed_event_removes_discovered_record" \
  "exec_policy_enforces_cwd_and_timeout" \
  "protocol_version_matches_grpc_release_line" \
  "append_record_writes_json_line" \
  "audit_event_uses_policy_subject_capability_and_context" \
  "init_config_then_explain_json_is_machine_readable"; do
  grep -q "$expected" "$TMP_DIR/test-list.txt"
done

echo "v0.8.1 integration coverage validation passed"
