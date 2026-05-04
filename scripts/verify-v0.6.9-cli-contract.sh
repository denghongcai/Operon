#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "v0.6.9 CLI contract validation currently requires Linux" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
DAEMON_PID=""
INIT_DAEMON_PID=""
cleanup() {
  if [[ -n "$DAEMON_PID" ]]; then
    kill "$DAEMON_PID" >/dev/null 2>&1 || true
    wait "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "$INIT_DAEMON_PID" ]]; then
    kill "$INIT_DAEMON_PID" >/dev/null 2>&1 || true
    wait "$INIT_DAEMON_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

WORKSPACE_DIR="$TMP_DIR/workspace"
CONFIG_PATH="$TMP_DIR/config.yaml"
STORE_PATH="$TMP_DIR/store.jsonl"
PORT="18869"

mkdir -p "$WORKSPACE_DIR"

cargo run -q -p operon-cli -- --version | grep -q "0.13.8"
cargo run -q -p operond -- --version | grep -q "0.13.8"

cat >"$CONFIG_PATH" <<YAML
version: 1

daemon:
  node_id: local
  grpc_listen: 127.0.0.1:$PORT
  workspace: $WORKSPACE_DIR
  advertise_lan: false
  store: $STORE_PATH

client:
  nodes:
    local:
      endpoint: grpc://127.0.0.1:$PORT

policy:
  subject: local-cli
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
    services: []
YAML

cargo run -q -p operond -- start --config "$CONFIG_PATH" >"$TMP_DIR/operond.log" 2>&1 &
DAEMON_PID="$!"

for _ in $(seq 1 50); do
  if cargo run -q -p operon-cli -- --config "$CONFIG_PATH" node ping local >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" node ping local \
  | grep -q "version=v0.13.8"

json_exec_output="$TMP_DIR/exec-run.json"
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" --json exec run local \
  --timeout-secs 10 \
  -- "printf json-contract" \
  >"$json_exec_output"
python3 - "$json_exec_output" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    record = json.load(handle)
assert record["status"] == "succeeded", record
assert record["exit_code"] == 0, record
PY

set +e
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" exec run local \
  --timeout-secs 10 \
  -- false \
  >"$TMP_DIR/exec-failed.out" 2>"$TMP_DIR/exec-failed.err"
failed_status="$?"
set -e
if [[ "$failed_status" -eq 0 ]]; then
  echo "expected failed remote exec to produce non-zero CLI exit" >&2
  exit 1
fi
grep -q "Failed" "$TMP_DIR/exec-failed.out"
grep -q "ended with status Failed" "$TMP_DIR/exec-failed.err"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" exec run local \
  --detach \
  --timeout-secs 10 \
  -- "printf log-contract" \
  >"$TMP_DIR/log-exec.txt"
log_exec_id="$(awk '{print $2}' "$TMP_DIR/log-exec.txt" | head -n1)"
for _ in $(seq 1 50); do
  cargo run -q -p operon-cli -- --config "$CONFIG_PATH" exec status local "$log_exec_id" \
    >"$TMP_DIR/log-exec-status.txt"
  if grep -q "Succeeded" "$TMP_DIR/log-exec-status.txt"; then
    break
  fi
  sleep 0.1
done
grep -q "Succeeded" "$TMP_DIR/log-exec-status.txt"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" --json exec logs local "$log_exec_id" \
  >"$TMP_DIR/exec-logs.json"
python3 - "$TMP_DIR/exec-logs.json" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    logs = json.load(handle)
payload = b"".join(bytes(log["data"]) for log in logs["logs"])
assert payload == b"log-contract", payload
PY

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" --quiet exec logs local "$log_exec_id" \
  >"$TMP_DIR/exec-logs-quiet.out"
test ! -s "$TMP_DIR/exec-logs-quiet.out"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" fs stat local:/ >/dev/null
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" --json audit show local \
  --capability fs:workspace \
  --action stat \
  --allowed true \
  --limit 1 \
  >"$TMP_DIR/audit-filtered.json"
python3 - "$TMP_DIR/audit-filtered.json" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    audit = json.load(handle)
events = audit["events"]
assert len(events) == 1, events
event = events[0]
assert event["capability"] == "fs:workspace", event
assert event["action"] == "stat", event
assert event["allowed"] is True, event
PY

INIT_DIR="$TMP_DIR/init"
INIT_CONFIG="$INIT_DIR/config.yaml"
cargo run -q -p operon-cli -- --quiet init config "$INIT_CONFIG"
test -s "$INIT_DIR/token"
test -s "$INIT_DIR/secrets.yaml"
sed -i "s#127.0.0.1:7789#127.0.0.1:18870#g" "$INIT_CONFIG"
sed -i "s#/workspace#$TMP_DIR/init-workspace#g" "$INIT_CONFIG"
mkdir -p "$TMP_DIR/init-workspace"
cargo run -q -p operond -- start --config "$INIT_CONFIG" >"$TMP_DIR/init-operond.log" 2>&1 &
INIT_DAEMON_PID="$!"
for _ in $(seq 1 50); do
  if cargo run -q -p operon-cli -- --config "$INIT_CONFIG" node ping local >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done
cargo run -q -p operon-cli -- --config "$INIT_CONFIG" node ping local | grep -q "ok=true"

echo "v0.6.9 CLI contract validation passed"
