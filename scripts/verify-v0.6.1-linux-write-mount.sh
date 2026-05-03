#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "SKIP: v0.6.1 Linux write mount validation requires Linux"
  exit 0
fi

if [[ ! -e /dev/fuse ]]; then
  echo "SKIP: v0.6.1 Linux write mount validation requires /dev/fuse"
  exit 0
fi

if ! command -v fusermount3 >/dev/null 2>&1 && ! command -v fusermount >/dev/null 2>&1; then
  echo "SKIP: v0.6.1 Linux write mount validation requires fusermount3 or fusermount"
  exit 0
fi

TMP_DIR="$(mktemp -d)"
WORKSPACE="$TMP_DIR/workspace"
STORE="$TMP_DIR/store.jsonl"
CONFIG="$TMP_DIR/config.yaml"

MOUNT_DIR="$TMP_DIR/mount"
MOUNT_LOG="$TMP_DIR/mount.log"
DENY_WRITE_WORKSPACE="$TMP_DIR/deny-write-workspace"
DENY_WRITE_STORE="$TMP_DIR/deny-write-store.jsonl"
DENY_WRITE_CONFIG="$TMP_DIR/deny-write-config.yaml"

DENY_WRITE_MOUNT="$TMP_DIR/deny-write-mount"
DENY_WRITE_LOG="$TMP_DIR/deny-write-mount.log"
DENY_DELETE_WORKSPACE="$TMP_DIR/deny-delete-workspace"
DENY_DELETE_STORE="$TMP_DIR/deny-delete-store.jsonl"
DENY_DELETE_CONFIG="$TMP_DIR/deny-delete-config.yaml"

DENY_DELETE_MOUNT="$TMP_DIR/deny-delete-mount"
DENY_DELETE_LOG="$TMP_DIR/deny-delete-mount.log"
DAEMON_PID=""
MOUNT_PID=""
DENY_WRITE_PID=""
DENY_WRITE_MOUNT_PID=""
DENY_DELETE_PID=""
DENY_DELETE_MOUNT_PID=""

unmount_dir() {
  local dir="$1"
  if mountpoint -q "$dir"; then
    if command -v fusermount3 >/dev/null 2>&1; then
      fusermount3 -u "$dir" >/dev/null 2>&1
    else
      fusermount -u "$dir" >/dev/null 2>&1
    fi
  fi
}

stop_mount() {
  local pid="$1"
  if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
    kill -INT "$pid" >/dev/null 2>&1
    wait "$pid" >/dev/null 2>&1 || true
  fi
}

stop_daemon() {
  local pid="$1"
  if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
    kill "$pid" >/dev/null 2>&1
    wait "$pid" >/dev/null 2>&1 || true
  fi
}

cleanup() {
  set +e
  stop_mount "$MOUNT_PID"
  stop_mount "$DENY_WRITE_MOUNT_PID"
  stop_mount "$DENY_DELETE_MOUNT_PID"
  unmount_dir "$MOUNT_DIR"
  unmount_dir "$DENY_WRITE_MOUNT"
  unmount_dir "$DENY_DELETE_MOUNT"
  stop_daemon "$DAEMON_PID"
  stop_daemon "$DENY_WRITE_PID"
  stop_daemon "$DENY_DELETE_PID"
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

wait_for_mount() {
  local dir="$1"
  for _ in $(seq 1 30); do
    if mountpoint -q "$dir"; then
      return 0
    fi
    sleep 1
  done
  mountpoint -q "$dir"
}

mkdir -p "$WORKSPACE" "$MOUNT_DIR"
write_config "$CONFIG" "write-node" 18791 "write-token" "$WORKSPACE" "$STORE" "v061-write" true true true

cargo run -q -p operond -- start --config "$CONFIG" >"$TMP_DIR/daemon.log" 2>&1 &
DAEMON_PID="$!"
wait_for_node "$CONFIG" write-node

cargo run -q -p operon-cli -- --config "$CONFIG" mount write-node:/ --to "$MOUNT_DIR" >"$MOUNT_LOG" 2>&1 &
MOUNT_PID="$!"
wait_for_mount "$MOUNT_DIR"

printf "created through mount" >"$MOUNT_DIR/new.txt"
grep -q "created through mount" "$MOUNT_DIR/new.txt"
cargo run -q -p operon-cli -- --config "$CONFIG" fs read write-node:/new.txt >"$TMP_DIR/new-read.txt"
grep -q "created through mount" "$TMP_DIR/new-read.txt"
printf "overwritten" >"$MOUNT_DIR/new.txt"
grep -q "^overwritten$" "$MOUNT_DIR/new.txt"
cargo run -q -p operon-cli -- --config "$CONFIG" fs read write-node:/new.txt >"$TMP_DIR/overwrite-read.txt"
grep -q "^overwritten$" "$TMP_DIR/overwrite-read.txt"

mkdir "$MOUNT_DIR/dir"
printf "abcdef" >"$MOUNT_DIR/dir/data.txt"
truncate -s 3 "$MOUNT_DIR/dir/data.txt"
grep -q "^abc$" "$MOUNT_DIR/dir/data.txt"
cargo run -q -p operon-cli -- --config "$CONFIG" fs read write-node:/dir/data.txt >"$TMP_DIR/truncated-read.txt"
grep -q "^abc$" "$TMP_DIR/truncated-read.txt"

mv "$MOUNT_DIR/dir/data.txt" "$MOUNT_DIR/dir/renamed.txt"
cargo run -q -p operon-cli -- --config "$CONFIG" fs read write-node:/dir/renamed.txt >"$TMP_DIR/renamed-read.txt"
grep -q "^abc$" "$TMP_DIR/renamed-read.txt"
if cargo run -q -p operon-cli -- --config "$CONFIG" fs stat write-node:/dir/data.txt >"$TMP_DIR/old-stat.log" 2>&1; then
  echo "expected old renamed path to be absent" >&2
  exit 1
fi

rm "$MOUNT_DIR/dir/renamed.txt"
rmdir "$MOUNT_DIR/dir"
if cargo run -q -p operon-cli -- --config "$CONFIG" fs stat write-node:/dir/renamed.txt >"$TMP_DIR/deleted-stat.log" 2>&1; then
  echo "expected deleted path to be absent" >&2
  exit 1
fi

stop_mount "$MOUNT_PID"
MOUNT_PID=""
if mountpoint -q "$MOUNT_DIR"; then
  echo "expected write mount to be unmounted after Ctrl-C" >&2
  exit 1
fi

mkdir -p "$DENY_WRITE_WORKSPACE" "$DENY_WRITE_MOUNT"
write_config "$DENY_WRITE_CONFIG" "deny-write" 18792 "deny-write-token" "$DENY_WRITE_WORKSPACE" "$DENY_WRITE_STORE" "v061-deny-write" true false true
cargo run -q -p operond -- start --config "$DENY_WRITE_CONFIG" >"$TMP_DIR/deny-write-daemon.log" 2>&1 &
DENY_WRITE_PID="$!"
wait_for_node "$DENY_WRITE_CONFIG" deny-write
cargo run -q -p operon-cli -- --config "$DENY_WRITE_CONFIG" mount deny-write:/ --to "$DENY_WRITE_MOUNT" >"$DENY_WRITE_LOG" 2>&1 &
DENY_WRITE_MOUNT_PID="$!"
wait_for_mount "$DENY_WRITE_MOUNT"
if sh -c "printf denied > '$DENY_WRITE_MOUNT/denied.txt'" >"$TMP_DIR/write-deny.log" 2>&1; then
  echo "expected denied write through FUSE mount to fail" >&2
  exit 1
fi
cargo run -q -p operon-cli -- --config "$DENY_WRITE_CONFIG" audit show deny-write --capability fs:workspace --action write-range --allowed false --limit 10 >"$TMP_DIR/write-deny-audit.log"
grep -Eq "fs:workspace[[:space:]]+write-range[[:space:]]+/.+[[:space:]]+false" "$TMP_DIR/write-deny-audit.log"
stop_mount "$DENY_WRITE_MOUNT_PID"
DENY_WRITE_MOUNT_PID=""

mkdir -p "$DENY_DELETE_WORKSPACE" "$DENY_DELETE_MOUNT"
printf "keep" >"$DENY_DELETE_WORKSPACE/keep.txt"
write_config "$DENY_DELETE_CONFIG" "deny-delete" 18793 "deny-delete-token" "$DENY_DELETE_WORKSPACE" "$DENY_DELETE_STORE" "v061-deny-delete" true true false
cargo run -q -p operond -- start --config "$DENY_DELETE_CONFIG" >"$TMP_DIR/deny-delete-daemon.log" 2>&1 &
DENY_DELETE_PID="$!"
wait_for_node "$DENY_DELETE_CONFIG" deny-delete
cargo run -q -p operon-cli -- --config "$DENY_DELETE_CONFIG" mount deny-delete:/ --to "$DENY_DELETE_MOUNT" >"$DENY_DELETE_LOG" 2>&1 &
DENY_DELETE_MOUNT_PID="$!"
wait_for_mount "$DENY_DELETE_MOUNT"
if rm "$DENY_DELETE_MOUNT/keep.txt" >"$TMP_DIR/delete-deny.log" 2>&1; then
  echo "expected denied delete through FUSE mount to fail" >&2
  exit 1
fi
if mv "$DENY_DELETE_MOUNT/keep.txt" "$DENY_DELETE_MOUNT/moved.txt" >"$TMP_DIR/rename-deny.log" 2>&1; then
  echo "expected denied rename through FUSE mount to fail" >&2
  exit 1
fi
cargo run -q -p operon-cli -- --config "$DENY_DELETE_CONFIG" audit show deny-delete --capability fs:workspace --action delete --allowed false --limit 10 >"$TMP_DIR/delete-deny-audit.log"
grep -Eq "fs:workspace[[:space:]]+delete[[:space:]]+/keep.txt[[:space:]]+false" "$TMP_DIR/delete-deny-audit.log"
cargo run -q -p operon-cli -- --config "$DENY_DELETE_CONFIG" audit show deny-delete --capability fs:workspace --action rename --allowed false --limit 10 >"$TMP_DIR/rename-deny-audit.log"
grep -Eq "fs:workspace[[:space:]]+rename[[:space:]]+/keep.txt -> /moved.txt[[:space:]]+false" "$TMP_DIR/rename-deny-audit.log"

echo "v0.6.1 Linux write mount validation passed"
