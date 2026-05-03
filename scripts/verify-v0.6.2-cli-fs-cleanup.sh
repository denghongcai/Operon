#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
ALLOW_WORKSPACE="$TMP_DIR/allow-workspace"
ALLOW_STORE="$TMP_DIR/allow-store.jsonl"
ALLOW_CONFIG="$TMP_DIR/allow-config.yaml"

DENY_WORKSPACE="$TMP_DIR/deny-workspace"
DENY_STORE="$TMP_DIR/deny-store.jsonl"
DENY_CONFIG="$TMP_DIR/deny-config.yaml"

ALLOW_PID=""
DENY_PID=""

cleanup() {
  set +e
  if [[ -n "$ALLOW_PID" ]] && kill -0 "$ALLOW_PID" >/dev/null 2>&1; then
    kill "$ALLOW_PID" >/dev/null 2>&1
    wait "$ALLOW_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "$DENY_PID" ]] && kill -0 "$DENY_PID" >/dev/null 2>&1; then
    kill "$DENY_PID" >/dev/null 2>&1
    wait "$DENY_PID" >/dev/null 2>&1 || true
  fi
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

mkdir -p "$ALLOW_WORKSPACE" "$DENY_WORKSPACE"
printf "deny" >"$DENY_WORKSPACE/existing.txt"

write_config "$ALLOW_CONFIG" "allow" 18794 "allow-token" "$ALLOW_WORKSPACE" "$ALLOW_STORE" "v062-allow" true true true
cargo run -q -p operond -- start --config "$ALLOW_CONFIG" >"$TMP_DIR/allow-daemon.log" 2>&1 &
ALLOW_PID="$!"
wait_for_node "$ALLOW_CONFIG" allow

cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs mkdir allow:/dir
cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs stat allow:/dir | grep -q "dir=true"

cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs truncate allow:/dir/file.txt --size 5
cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs stat allow:/dir/file.txt | grep -q "size=5"
cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs write allow:/dir/file.txt --content "abcdef"
cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs truncate allow:/dir/file.txt --size 3
cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs read allow:/dir/file.txt >"$TMP_DIR/truncated.txt"
grep -q "^abc$" "$TMP_DIR/truncated.txt"

cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs rename allow:/dir/file.txt allow:/dir/renamed.txt
cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs read allow:/dir/renamed.txt >"$TMP_DIR/renamed.txt"
grep -q "^abc$" "$TMP_DIR/renamed.txt"
if cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs stat allow:/dir/file.txt >"$TMP_DIR/old-stat.log" 2>&1; then
  echo "expected old renamed path to be absent" >&2
  exit 1
fi

cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs rm allow:/dir/renamed.txt
cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs rm allow:/dir
if cargo run -q -p operon-cli -- --config "$ALLOW_CONFIG" fs stat allow:/dir >"$TMP_DIR/deleted-stat.log" 2>&1; then
  echo "expected removed directory to be absent" >&2
  exit 1
fi

write_config "$DENY_CONFIG" "deny" 18795 "deny-token" "$DENY_WORKSPACE" "$DENY_STORE" "v062-deny" true false false
cargo run -q -p operond -- start --config "$DENY_CONFIG" >"$TMP_DIR/deny-daemon.log" 2>&1 &
DENY_PID="$!"
wait_for_node "$DENY_CONFIG" deny

if cargo run -q -p operon-cli -- --config "$DENY_CONFIG" fs mkdir deny:/blocked-dir >"$TMP_DIR/deny-mkdir.log" 2>&1; then
  echo "expected denied mkdir to fail" >&2
  exit 1
fi
if cargo run -q -p operon-cli -- --config "$DENY_CONFIG" fs truncate deny:/blocked.txt --size 1 >"$TMP_DIR/deny-truncate.log" 2>&1; then
  echo "expected denied truncate to fail" >&2
  exit 1
fi
if cargo run -q -p operon-cli -- --config "$DENY_CONFIG" fs rm deny:/existing.txt >"$TMP_DIR/deny-rm.log" 2>&1; then
  echo "expected denied rm to fail" >&2
  exit 1
fi
if cargo run -q -p operon-cli -- --config "$DENY_CONFIG" fs rename deny:/existing.txt deny:/moved.txt >"$TMP_DIR/deny-rename.log" 2>&1; then
  echo "expected denied rename to fail" >&2
  exit 1
fi

cargo run -q -p operon-cli -- --config "$DENY_CONFIG" audit show deny --capability fs:workspace --action mkdir --allowed false --limit 10 >"$TMP_DIR/deny-mkdir-audit.log"
grep -Eq "fs:workspace[[:space:]]+mkdir[[:space:]]+/blocked-dir[[:space:]]+false" "$TMP_DIR/deny-mkdir-audit.log"
cargo run -q -p operon-cli -- --config "$DENY_CONFIG" audit show deny --capability fs:workspace --action truncate --allowed false --limit 10 >"$TMP_DIR/deny-truncate-audit.log"
grep -Eq "fs:workspace[[:space:]]+truncate[[:space:]]+/blocked.txt[[:space:]]+false" "$TMP_DIR/deny-truncate-audit.log"
cargo run -q -p operon-cli -- --config "$DENY_CONFIG" audit show deny --capability fs:workspace --action delete --allowed false --limit 10 >"$TMP_DIR/deny-rm-audit.log"
grep -Eq "fs:workspace[[:space:]]+delete[[:space:]]+/existing.txt[[:space:]]+false" "$TMP_DIR/deny-rm-audit.log"
cargo run -q -p operon-cli -- --config "$DENY_CONFIG" audit show deny --capability fs:workspace --action rename --allowed false --limit 10 >"$TMP_DIR/deny-rename-audit.log"
grep -Eq "fs:workspace[[:space:]]+rename[[:space:]]+/existing.txt -> /moved.txt[[:space:]]+false" "$TMP_DIR/deny-rename-audit.log"

echo "v0.6.2 CLI fs cleanup validation passed"
