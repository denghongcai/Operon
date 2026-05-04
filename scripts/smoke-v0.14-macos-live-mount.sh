#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "v0.14 macOS live mount smoke requires macOS" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
WORKSPACE="$TMP_DIR/workspace"
STORE="$TMP_DIR/store.jsonl"
CONFIG="$TMP_DIR/config.yaml"
MOUNT_DIR="$TMP_DIR/mount"
MOUNT_LOG="$TMP_DIR/mount.log"
DAEMON_LOG="$TMP_DIR/daemon.log"
DAEMON_PID=""
MOUNT_PID=""

cleanup() {
  set +e
  if [[ -n "$MOUNT_PID" ]] && kill -0 "$MOUNT_PID" >/dev/null 2>&1; then
    kill -INT "$MOUNT_PID" >/dev/null 2>&1
    wait "$MOUNT_PID" >/dev/null 2>&1 || true
  fi
  if mount | grep -F " on $MOUNT_DIR " >/dev/null 2>&1; then
    umount "$MOUNT_DIR" >/dev/null 2>&1 || true
  fi
  if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
    kill "$DAEMON_PID" >/dev/null 2>&1
    wait "$DAEMON_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

write_config() {
  cat >"$CONFIG" <<YAML
version: 1
daemon:
  node_id: macos-live
  grpc_listen: 127.0.0.1:18841
  workspace: $WORKSPACE
  store: $STORE
  auth:
    token: macos-live-token
client:
  nodes:
    macos-live:
      endpoint: grpc://127.0.0.1:18841
      auth:
        token: macos-live-token
policy:
  subject: v014-macos-live-smoke
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
    max_timeout_secs: 30
    env_allowlist: []
    allowed_secrets: []
  service:
    services: []
YAML
}

wait_for_node() {
  for _ in $(seq 1 30); do
    if cargo run -q -p operon-cli -- --config "$CONFIG" node ping macos-live >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  cargo run -q -p operon-cli -- --config "$CONFIG" node ping macos-live
}

wait_for_mount() {
  for _ in $(seq 1 30); do
    if [[ -f "$MOUNT_DIR/seed.txt" ]]; then
      return 0
    fi
    sleep 1
  done
  echo "mount did not expose seed file" >&2
  cat "$MOUNT_LOG" >&2 || true
  return 1
}

mkdir -p "$WORKSPACE" "$MOUNT_DIR"
printf "seed" >"$WORKSPACE/seed.txt"
write_config

cargo run -q -p operond -- start --config "$CONFIG" >"$DAEMON_LOG" 2>&1 &
DAEMON_PID="$!"
wait_for_node

cargo run -q -p operon-cli -- --config "$CONFIG" mount macos-live:/ --to "$MOUNT_DIR" >"$MOUNT_LOG" 2>&1 &
MOUNT_PID="$!"
wait_for_mount

grep -q "^seed$" "$MOUNT_DIR/seed.txt"

printf "created through macos mount" >"$MOUNT_DIR/new.txt"
cargo run -q -p operon-cli -- --config "$CONFIG" fs read macos-live:/new.txt >"$TMP_DIR/new-read.txt"
grep -q "^created through macos mount$" "$TMP_DIR/new-read.txt"

mkdir "$MOUNT_DIR/dir"
printf "abcdef" >"$MOUNT_DIR/dir/data.txt"
truncate -s 3 "$MOUNT_DIR/dir/data.txt"
grep -q "^abc$" "$MOUNT_DIR/dir/data.txt"
mv "$MOUNT_DIR/dir/data.txt" "$MOUNT_DIR/dir/renamed.txt"
cargo run -q -p operon-cli -- --config "$CONFIG" fs read macos-live:/dir/renamed.txt >"$TMP_DIR/renamed-read.txt"
grep -q "^abc$" "$TMP_DIR/renamed-read.txt"
rm "$MOUNT_DIR/dir/renamed.txt"
rmdir "$MOUNT_DIR/dir"

echo "v0.14 macOS live mount smoke passed"
