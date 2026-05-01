#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

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

SETUP_DIR="$TMP_DIR/setup"
WORKSPACE_DIR="$TMP_DIR/workspace"
STORE_PATH="$TMP_DIR/store.jsonl"
PORT="18789"

mkdir -p "$WORKSPACE_DIR"

cargo run -q -p operon-cli -- onboard \
  --yes \
  --role both \
  --output-dir "$SETUP_DIR" \
  --node-id onboard-node \
  --workspace "$WORKSPACE_DIR" \
  --listen "127.0.0.1:$PORT" \
  --token onboard-token

test -f "$SETUP_DIR/config.yaml"
test -f "$SETUP_DIR/token"
test -f "$SETUP_DIR/daemon-command.txt"
grep -q "endpoint: grpc://127.0.0.1:$PORT" "$SETUP_DIR/config.yaml"
grep -q "token_file: token" "$SETUP_DIR/config.yaml"
grep -q "port: $PORT" "$SETUP_DIR/config.yaml"
grep -q -- "operond start --config $SETUP_DIR/config.yaml" "$SETUP_DIR/daemon-command.txt"

cargo run -q -p operond -- start \
  --config "$SETUP_DIR/config.yaml" \
  >"$TMP_DIR/operond.log" 2>&1 &
DAEMON_PID="$!"

for _ in $(seq 1 50); do
  if cargo run -q -p operon-cli -- --config "$SETUP_DIR/config.yaml" node ping onboard-node >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

cargo run -q -p operon-cli -- --config "$SETUP_DIR/config.yaml" node ping onboard-node | grep -q "ok=true"
cargo run -q -p operon-cli -- --config "$SETUP_DIR/config.yaml" capability list onboard-node | grep -q "onboard-node/fs"
cargo run -q -p operon-cli -- --config "$SETUP_DIR/config.yaml" fs write onboard-node:/hello.txt --content "hello onboard" | grep -q "bytes_written="
cargo run -q -p operon-cli -- --config "$SETUP_DIR/config.yaml" fs read onboard-node:/hello.txt | grep -q "hello onboard"
cargo run -q -p operon-cli -- --config "$SETUP_DIR/config.yaml" audit show onboard-node --limit 10 | grep -q "fs"

echo "v0.6.4 onboard validation passed"
