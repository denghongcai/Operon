#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
ALLOW_WORKSPACE="$TMP_DIR/allow-workspace"
ALLOW_STORE="$TMP_DIR/allow-store.jsonl"
ALLOW_POLICY="$TMP_DIR/allow-policy.yaml"
ALLOW_NODES="$TMP_DIR/allow-nodes.yaml"
DENY_WORKSPACE="$TMP_DIR/deny-workspace"
DENY_STORE="$TMP_DIR/deny-store.jsonl"
DENY_POLICY="$TMP_DIR/deny-policy.yaml"
DENY_NODES="$TMP_DIR/deny-nodes.yaml"
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

write_policy() {
  local path="$1"
  local subject="$2"
  local read="$3"
  local write="$4"
  local delete="$5"
  cat >"$path" <<YAML
subject: $subject

fs:
  mounts:
    - name: workspace
      path: /
      permissions:
        read: $read
        write: $write
        delete: $delete

job:
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

write_nodes() {
  local path="$1"
  local node_id="$2"
  local port="$3"
  local token="$4"
  cat >"$path" <<YAML
nodes:
  $node_id:
    endpoint: grpc://127.0.0.1:$port
    token: $token
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

write_policy "$ALLOW_POLICY" "v062-allow" true true true
write_nodes "$ALLOW_NODES" "allow" 18794 "allow-token"
cargo run -q -p operond -- start \
  --grpc-listen 127.0.0.1:18794 \
  --node-id allow \
  --workspace "$ALLOW_WORKSPACE" \
  --policy "$ALLOW_POLICY" \
  --auth-token allow-token \
  --store "$ALLOW_STORE" >"$TMP_DIR/allow-daemon.log" 2>&1 &
ALLOW_PID="$!"
wait_for_node "$ALLOW_NODES" allow

cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs mkdir allow:/dir
cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs stat allow:/dir | grep -q "dir=true"

cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs truncate allow:/dir/file.txt --size 5
cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs stat allow:/dir/file.txt | grep -q "size=5"
cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs write allow:/dir/file.txt --content "abcdef"
cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs truncate allow:/dir/file.txt --size 3
cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs read allow:/dir/file.txt >"$TMP_DIR/truncated.txt"
grep -q "^abc$" "$TMP_DIR/truncated.txt"

cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs rename allow:/dir/file.txt allow:/dir/renamed.txt
cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs read allow:/dir/renamed.txt >"$TMP_DIR/renamed.txt"
grep -q "^abc$" "$TMP_DIR/renamed.txt"
if cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs stat allow:/dir/file.txt >"$TMP_DIR/old-stat.log" 2>&1; then
  echo "expected old renamed path to be absent" >&2
  exit 1
fi

cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs rm allow:/dir/renamed.txt
cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs rm allow:/dir
if cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs stat allow:/dir >"$TMP_DIR/deleted-stat.log" 2>&1; then
  echo "expected removed directory to be absent" >&2
  exit 1
fi

write_policy "$DENY_POLICY" "v062-deny" true false false
write_nodes "$DENY_NODES" "deny" 18795 "deny-token"
cargo run -q -p operond -- start \
  --grpc-listen 127.0.0.1:18795 \
  --node-id deny \
  --workspace "$DENY_WORKSPACE" \
  --policy "$DENY_POLICY" \
  --auth-token deny-token \
  --store "$DENY_STORE" >"$TMP_DIR/deny-daemon.log" 2>&1 &
DENY_PID="$!"
wait_for_node "$DENY_NODES" deny

if cargo run -q -p operon-cli -- --config "$DENY_NODES" fs mkdir deny:/blocked-dir >"$TMP_DIR/deny-mkdir.log" 2>&1; then
  echo "expected denied mkdir to fail" >&2
  exit 1
fi
if cargo run -q -p operon-cli -- --config "$DENY_NODES" fs truncate deny:/blocked.txt --size 1 >"$TMP_DIR/deny-truncate.log" 2>&1; then
  echo "expected denied truncate to fail" >&2
  exit 1
fi
if cargo run -q -p operon-cli -- --config "$DENY_NODES" fs rm deny:/existing.txt >"$TMP_DIR/deny-rm.log" 2>&1; then
  echo "expected denied rm to fail" >&2
  exit 1
fi
if cargo run -q -p operon-cli -- --config "$DENY_NODES" fs rename deny:/existing.txt deny:/moved.txt >"$TMP_DIR/deny-rename.log" 2>&1; then
  echo "expected denied rename to fail" >&2
  exit 1
fi

cargo run -q -p operon-cli -- --config "$DENY_NODES" audit show deny --capability fs:workspace --action mkdir --allowed false --limit 10 >"$TMP_DIR/deny-mkdir-audit.log"
grep -Eq "fs:workspace[[:space:]]+mkdir[[:space:]]+/blocked-dir[[:space:]]+false" "$TMP_DIR/deny-mkdir-audit.log"
cargo run -q -p operon-cli -- --config "$DENY_NODES" audit show deny --capability fs:workspace --action truncate --allowed false --limit 10 >"$TMP_DIR/deny-truncate-audit.log"
grep -Eq "fs:workspace[[:space:]]+truncate[[:space:]]+/blocked.txt[[:space:]]+false" "$TMP_DIR/deny-truncate-audit.log"
cargo run -q -p operon-cli -- --config "$DENY_NODES" audit show deny --capability fs:workspace --action delete --allowed false --limit 10 >"$TMP_DIR/deny-rm-audit.log"
grep -Eq "fs:workspace[[:space:]]+delete[[:space:]]+/existing.txt[[:space:]]+false" "$TMP_DIR/deny-rm-audit.log"
cargo run -q -p operon-cli -- --config "$DENY_NODES" audit show deny --capability fs:workspace --action rename --allowed false --limit 10 >"$TMP_DIR/deny-rename-audit.log"
grep -Eq "fs:workspace[[:space:]]+rename[[:space:]]+/existing.txt -> /moved.txt[[:space:]]+false" "$TMP_DIR/deny-rename-audit.log"

echo "v0.6.2 CLI fs cleanup validation passed"
