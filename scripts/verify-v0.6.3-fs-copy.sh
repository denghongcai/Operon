#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
ALLOW_WORKSPACE="$TMP_DIR/allow-workspace"
ALLOW_STORE="$TMP_DIR/allow-store.jsonl"
ALLOW_POLICY="$TMP_DIR/allow-policy.yaml"
ALLOW_NODES="$TMP_DIR/allow-nodes.yaml"
DENY_READ_WORKSPACE="$TMP_DIR/deny-read-workspace"
DENY_READ_STORE="$TMP_DIR/deny-read-store.jsonl"
DENY_READ_POLICY="$TMP_DIR/deny-read-policy.yaml"
DENY_READ_NODES="$TMP_DIR/deny-read-nodes.yaml"
DENY_WRITE_WORKSPACE="$TMP_DIR/deny-write-workspace"
DENY_WRITE_STORE="$TMP_DIR/deny-write-store.jsonl"
DENY_WRITE_POLICY="$TMP_DIR/deny-write-policy.yaml"
DENY_WRITE_NODES="$TMP_DIR/deny-write-nodes.yaml"
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

start_daemon() {
  local nodes="$1"
  local node_id="$2"
  local port="$3"
  local token="$4"
  local workspace="$5"
  local policy="$6"
  local store="$7"
  local log="$8"

  cargo run -q -p operond -- start \
    --grpc-listen "127.0.0.1:$port" \
    --node-id "$node_id" \
    --workspace "$workspace" \
    --policy "$policy" \
    --auth-token "$token" \
    --store "$store" >"$log" 2>&1 &
  wait_for_node "$nodes" "$node_id" >&2
  echo "$!"
}

mkdir -p "$ALLOW_WORKSPACE/src" "$DENY_READ_WORKSPACE/src" "$DENY_WRITE_WORKSPACE/src"
printf "copy through daemon" >"$ALLOW_WORKSPACE/src/input.txt"
printf "deny read" >"$DENY_READ_WORKSPACE/src/input.txt"
printf "deny write" >"$DENY_WRITE_WORKSPACE/src/input.txt"

write_policy "$ALLOW_POLICY" "v063-allow" true true true
write_nodes "$ALLOW_NODES" "allow" 18796 "allow-token"
ALLOW_PID="$(start_daemon "$ALLOW_NODES" allow 18796 allow-token "$ALLOW_WORKSPACE" "$ALLOW_POLICY" "$ALLOW_STORE" "$TMP_DIR/allow-daemon.log")"

cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs copy allow:/src/input.txt allow:/src/copied.txt >"$TMP_DIR/copy.log"
grep -q "bytes_copied=19" "$TMP_DIR/copy.log"
cargo run -q -p operon-cli -- --config "$ALLOW_NODES" fs read allow:/src/copied.txt >"$TMP_DIR/copied.txt"
grep -q "^copy through daemon$" "$TMP_DIR/copied.txt"
cargo run -q -p operon-cli -- --config "$ALLOW_NODES" audit show allow --capability fs:workspace --action copy --allowed true --resource "/src/input.txt -> /src/copied.txt" --limit 5 >"$TMP_DIR/copy-audit.log"
grep -Eq "fs:workspace[[:space:]]+copy[[:space:]]+/src/input.txt -> /src/copied.txt[[:space:]]+true" "$TMP_DIR/copy-audit.log"

write_policy "$DENY_READ_POLICY" "v063-deny-read" false true true
write_nodes "$DENY_READ_NODES" "deny-read" 18797 "deny-read-token"
DENY_READ_PID="$(start_daemon "$DENY_READ_NODES" deny-read 18797 deny-read-token "$DENY_READ_WORKSPACE" "$DENY_READ_POLICY" "$DENY_READ_STORE" "$TMP_DIR/deny-read-daemon.log")"

if cargo run -q -p operon-cli -- --config "$DENY_READ_NODES" fs copy deny-read:/src/input.txt deny-read:/src/copied.txt >"$TMP_DIR/deny-read-copy.log" 2>&1; then
  echo "expected denied source read copy to fail" >&2
  exit 1
fi
cargo run -q -p operon-cli -- --config "$DENY_READ_NODES" audit show deny-read --capability fs:workspace --action copy --allowed false --limit 5 >"$TMP_DIR/deny-read-audit.log"
grep -Eq "fs:workspace[[:space:]]+copy[[:space:]]+/src/input.txt -> /src/copied.txt[[:space:]]+false" "$TMP_DIR/deny-read-audit.log"

write_policy "$DENY_WRITE_POLICY" "v063-deny-write" true false true
write_nodes "$DENY_WRITE_NODES" "deny-write" 18798 "deny-write-token"
DENY_WRITE_PID="$(start_daemon "$DENY_WRITE_NODES" deny-write 18798 deny-write-token "$DENY_WRITE_WORKSPACE" "$DENY_WRITE_POLICY" "$DENY_WRITE_STORE" "$TMP_DIR/deny-write-daemon.log")"

if cargo run -q -p operon-cli -- --config "$DENY_WRITE_NODES" fs copy deny-write:/src/input.txt deny-write:/src/copied.txt >"$TMP_DIR/deny-write-copy.log" 2>&1; then
  echo "expected denied target write copy to fail" >&2
  exit 1
fi
cargo run -q -p operon-cli -- --config "$DENY_WRITE_NODES" audit show deny-write --capability fs:workspace --action copy --allowed false --limit 5 >"$TMP_DIR/deny-write-audit.log"
grep -Eq "fs:workspace[[:space:]]+copy[[:space:]]+/src/input.txt -> /src/copied.txt[[:space:]]+false" "$TMP_DIR/deny-write-audit.log"

echo "v0.6.3 fs copy validation passed"
