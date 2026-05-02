#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "v0.6.7 runtime validation currently requires Linux" >&2
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

WORKSPACE_DIR="$TMP_DIR/workspace"
CONFIG_PATH="$TMP_DIR/config.yaml"
STORE_PATH="$TMP_DIR/store.jsonl"
PORT="18867"

mkdir -p "$WORKSPACE_DIR"

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
      provider: manual

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
  job:
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

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" node ping local | grep -q "ok=true"
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" capability list local >"$TMP_DIR/capabilities.txt"
grep -q "local/fs:workspace" "$TMP_DIR/capabilities.txt"

printf 'streamed fs content\n' >"$TMP_DIR/fs-stream-input.txt"
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" fs write local:/streamed.txt \
  --file "$TMP_DIR/fs-stream-input.txt" >/dev/null
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" fs read local:/streamed.txt \
  --output "$TMP_DIR/fs-stream-output.txt"
cmp "$TMP_DIR/fs-stream-input.txt" "$TMP_DIR/fs-stream-output.txt"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" job run local \
  --detach \
  --timeout-secs 10 \
  -- "cat > stdin-streamed.txt" \
  >"$TMP_DIR/stdin-job.txt"
stdin_job_id="$(awk '{print $2}' "$TMP_DIR/stdin-job.txt" | head -n1)"
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" job stdin local "$stdin_job_id" \
  --content "stdin stream content"
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" job stdin local "$stdin_job_id" --close
for _ in $(seq 1 50); do
  cargo run -q -p operon-cli -- --config "$CONFIG_PATH" job status local "$stdin_job_id" \
    >"$TMP_DIR/stdin-status.txt"
  if grep -q "Succeeded" "$TMP_DIR/stdin-status.txt"; then
    break
  fi
  sleep 0.1
done
grep -q "Succeeded" "$TMP_DIR/stdin-status.txt"
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" fs read local:/stdin-streamed.txt \
  --output "$TMP_DIR/stdin-output.txt"
grep -q "stdin stream content" "$TMP_DIR/stdin-output.txt"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" job run local \
  --detach \
  --timeout-secs 60 \
  -- 'sleep 30 & echo $! > child.pid; wait' \
  >"$TMP_DIR/process-group-job.txt"
process_job_id="$(awk '{print $2}' "$TMP_DIR/process-group-job.txt" | head -n1)"

for _ in $(seq 1 50); do
  if [[ -f "$WORKSPACE_DIR/child.pid" ]]; then
    break
  fi
  sleep 0.1
done
test -f "$WORKSPACE_DIR/child.pid"
child_pid="$(cat "$WORKSPACE_DIR/child.pid")"
kill -0 "$child_pid"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" job cancel local "$process_job_id" >/dev/null
for _ in $(seq 1 50); do
  cargo run -q -p operon-cli -- --config "$CONFIG_PATH" job status local "$process_job_id" \
    >"$TMP_DIR/process-group-status.txt"
  if grep -q "Cancelled" "$TMP_DIR/process-group-status.txt"; then
    break
  fi
  sleep 0.1
done
grep -q "Cancelled" "$TMP_DIR/process-group-status.txt"

for _ in $(seq 1 50); do
  if ! kill -0 "$child_pid" >/dev/null 2>&1; then
    child_pid=""
    break
  fi
  child_state="$(ps -o stat= -p "$child_pid" 2>/dev/null | awk '{print $1}')"
  if [[ "$child_state" == Z* ]]; then
    child_pid=""
    break
  fi
  sleep 0.1
done
if [[ -n "$child_pid" ]] && kill -0 "$child_pid" >/dev/null 2>&1; then
  echo "expected process-group child $child_pid to be terminated" >&2
  ps -o pid,ppid,pgid,stat,command -p "$child_pid" >&2 || true
  exit 1
fi

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" job run local \
  --detach \
  --timeout-secs 10 \
  -- "printf '\\377\\000A'" \
  >"$TMP_DIR/binary-log-job.txt"
binary_job_id="$(awk '{print $2}' "$TMP_DIR/binary-log-job.txt" | head -n1)"

for _ in $(seq 1 50); do
  cargo run -q -p operon-cli -- --config "$CONFIG_PATH" job status local "$binary_job_id" \
    >"$TMP_DIR/binary-log-status.txt"
  if grep -q "Succeeded" "$TMP_DIR/binary-log-status.txt"; then
    break
  fi
  sleep 0.1
done
grep -q "Succeeded" "$TMP_DIR/binary-log-status.txt"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" job logs local "$binary_job_id" --stream \
  >"$TMP_DIR/binary-log-output.bin"
printf '\377\000A' >"$TMP_DIR/binary-log-expected.bin"
cmp "$TMP_DIR/binary-log-expected.bin" "$TMP_DIR/binary-log-output.bin"

cargo run -q -p operon-cli -- --config "$CONFIG_PATH" job list local >"$TMP_DIR/jobs.txt"
grep -q "$binary_job_id" "$TMP_DIR/jobs.txt"
cargo run -q -p operon-cli -- --config "$CONFIG_PATH" audit list local >"$TMP_DIR/audit.txt"
grep -q "write-stream" "$TMP_DIR/audit.txt"
grep -q "job:default" "$TMP_DIR/audit.txt"

echo "v0.6.7/v0.6.8/v0.6.12 runtime validation passed"
