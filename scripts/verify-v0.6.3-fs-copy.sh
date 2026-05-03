#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
ALLOW_WORKSPACE="$TMP_DIR/allow-workspace"
ALLOW_STORE="$TMP_DIR/allow-store.jsonl"
ALLOW_CONFIG="$TMP_DIR/allow-config.yaml"

DENY_READ_WORKSPACE="$TMP_DIR/deny-read-workspace"
DENY_READ_STORE="$TMP_DIR/deny-read-store.jsonl"
DENY_READ_CONFIG="$TMP_DIR/deny-read-config.yaml"

DENY_WRITE_WORKSPACE="$TMP_DIR/deny-write-workspace"
DENY_WRITE_STORE="$TMP_DIR/deny-write-store.jsonl"
DENY_WRITE_CONFIG="$TMP_DIR/deny-write-config.yaml"

ALLOW_PID=""
DENY_READ_PID=""
DENY_WRITE_PID=""

cleanup() {
  set +e
  for pid in "$ALLOW_PID" "$DENY_READ_PID" "$DENY_WRITE_PID"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1
      wait "$pid" >/dev/null 2>&1 || true
    fi
  done
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

write_config() {
  local path="$1"
  local node_id="$2"
  local port="$3"
  local token="$4"
  local workspace="$5"
  local store="$6"
  local subject="$7"
  local read="$8"
  local write="$9"
  local delete="${10}"
  cat >"$path" <<YAML
version: 1
daemon:
  node_id: $node_id
  grpc_listen: 127.0.0.1:$port
  workspace: $workspace
  store: $store
  auth:
    token: $token
client:
  nodes:
    $node_id:
      endpoint: grpc://127.0.0.1:$port
      auth:
        token: $token
policy:
  subject: $subject
  fs:
    mounts:
      - name: workspace
        path: /
        permissions:
          read: $read
          write: $write
          delete: $delete
  exec:
    allowed_cwds:
      - /
    default_timeout_secs: 30
    max_timeout_secs: 30
    env_allowlist: []
    allowed_secrets: []
  service:
    services: []
YAML
}

wait_for_node() {
  local nodes="$1"
  local node_id="$2"
  for _ in $(seq 1 30); do
    if cargo run -q -p operon-cli -- --config "$nodes" node ping "$node_id" >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  cargo run -q -p operon-cli -- --config "$nodes" node ping "$node_id"
}

start_daemon() {
  local config="$1"
  local node_id="$2"
  local log="$3"

  cargo run -q -p operond -- start --config "$config" >"$log" 2>&1 &
  wait_for_node "$config" "$node_id" >&2
  echo "$!"
}

mkdir -p "$ALLOW_WORKSPACE/src" "$DENY_READ_WORKSPACE/src" "$DENY_WRITE_WORKSPACE/src"
printf "copy through daemon" >"$ALLOW_WORKSPACE/src/input.txt"
printf "deny read" >"$DENY_READ_WORKSPACE/src/input.txt"
printf "deny write" >"$DENY_WRITE_WORKSPACE/src/input.txt"

write_config "$ALLOW_CONFIG" "allow" 18796 "allow-token" "$ALLOW_WORKSPACE" "$ALLOW_STORE" "v063-allow" true true true
ALLOW_PID="$(start_daemon "$ALLOW_CONFIG" allow "$TMP_DIR/allow-daemon.log")"

cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs copy allow:/src/input.txt allow:/src/copied.txt >"$TMP_DIR/copy.log"
grep -q "bytes_copied=19" "$TMP_DIR/copy.log"
cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs read allow:/src/copied.txt >"$TMP_DIR/copied.txt"
grep -q "^copy through daemon$" "$TMP_DIR/copied.txt"
cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" audit show allow --capability fs:workspace --action copy --allowed true --resource "/src/input.txt -> /src/copied.txt" --limit 5 >"$TMP_DIR/copy-audit.log"
grep -Eq "fs:workspace[[:space:]]+copy[[:space:]]+/src/input.txt -> /src/copied.txt[[:space:]]+true" "$TMP_DIR/copy-audit.log"

write_config "$DENY_READ_CONFIG" "deny-read" 18797 "deny-read-token" "$DENY_READ_WORKSPACE" "$DENY_READ_STORE" "v063-deny-read" false true true
DENY_READ_PID="$(start_daemon "$DENY_READ_CONFIG" deny-read "$TMP_DIR/deny-read-daemon.log")"

if cargo run -q -p operon-cli -- --config "$DENY_READ_CONFIG" fs copy deny-read:/src/input.txt deny-read:/src/copied.txt >"$TMP_DIR/deny-read-copy.log" 2>&1; then
  echo "expected denied source read copy to fail" >&2
  exit 1
fi
cargo run -q -p operon-cli -- --config "$DENY_READ_CONFIG" audit show deny-read --capability fs:workspace --action copy --allowed false --limit 5 >"$TMP_DIR/deny-read-audit.log"
grep -Eq "fs:workspace[[:space:]]+copy[[:space:]]+/src/input.txt -> /src/copied.txt[[:space:]]+false" "$TMP_DIR/deny-read-audit.log"

write_config "$DENY_WRITE_CONFIG" "deny-write" 18798 "deny-write-token" "$DENY_WRITE_WORKSPACE" "$DENY_WRITE_STORE" "v063-deny-write" true false true
DENY_WRITE_PID="$(start_daemon "$DENY_WRITE_CONFIG" deny-write "$TMP_DIR/deny-write-daemon.log")"

if cargo run -q -p operon-cli -- --config "$DENY_WRITE_CONFIG" fs copy deny-write:/src/input.txt deny-write:/src/copied.txt >"$TMP_DIR/deny-write-copy.log" 2>&1; then
  echo "expected denied target write copy to fail" >&2
  exit 1
fi
cargo run -q -p operon-cli -- --config "$DENY_WRITE_CONFIG" audit show deny-write --capability fs:workspace --action copy --allowed false --limit 5 >"$TMP_DIR/deny-write-audit.log"
grep -Eq "fs:workspace[[:space:]]+copy[[:space:]]+/src/input.txt -> /src/copied.txt[[:space:]]+false" "$TMP_DIR/deny-write-audit.log"

echo "v0.6.3 fs copy validation passed"
