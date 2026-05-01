#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "SKIP: v0.6 Linux mount validation requires Linux"
  exit 0
fi

if [[ ! -e /dev/fuse ]]; then
  echo "SKIP: v0.6 Linux mount validation requires /dev/fuse"
  exit 0
fi

if ! command -v fusermount3 >/dev/null 2>&1 && ! command -v fusermount >/dev/null 2>&1; then
  echo "SKIP: v0.6 Linux mount validation requires fusermount3 or fusermount"
  exit 0
fi

TMP_DIR="$(mktemp -d)"
MOUNT_DIR="$TMP_DIR/mount"
DENY_WORKSPACE="$TMP_DIR/deny-workspace"
DENY_STORE="$TMP_DIR/deny-store.jsonl"
DENY_POLICY="$TMP_DIR/deny-policy.yaml"
DENY_NODES="$TMP_DIR/deny-nodes.yaml"
MOUNT_LOG="$TMP_DIR/mount.log"
DENY_LOG="$TMP_DIR/deny-mount.log"
MOUNT_PID=""
DENY_PID=""

cleanup() {
  set +e
  if [[ -n "$MOUNT_PID" ]] && kill -0 "$MOUNT_PID" >/dev/null 2>&1; then
    kill -INT "$MOUNT_PID" >/dev/null 2>&1
    wait "$MOUNT_PID" >/dev/null 2>&1
  fi
  if mountpoint -q "$MOUNT_DIR"; then
    if command -v fusermount3 >/dev/null 2>&1; then
      fusermount3 -u "$MOUNT_DIR" >/dev/null 2>&1
    else
      fusermount -u "$MOUNT_DIR" >/dev/null 2>&1
    fi
  fi
  if [[ -n "$DENY_PID" ]] && kill -0 "$DENY_PID" >/dev/null 2>&1; then
    kill "$DENY_PID" >/dev/null 2>&1
    wait "$DENY_PID" >/dev/null 2>&1
  fi
  docker compose down >/dev/null 2>&1
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

docker compose up -d --build node-a node-b

for _ in $(seq 1 30); do
  if cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml node ping node-a >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml node ping node-a

cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs write node-a:/mount-v06.txt --content "hello from v0.6 mount"

mkdir -p "$MOUNT_DIR"
cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml mount node-a:/ --to "$MOUNT_DIR" >"$MOUNT_LOG" 2>&1 &
MOUNT_PID="$!"

for _ in $(seq 1 30); do
  if mountpoint -q "$MOUNT_DIR" && grep -q "hello from v0.6 mount" "$MOUNT_DIR/mount-v06.txt" 2>/dev/null; then
    break
  fi
  sleep 1
done

mountpoint -q "$MOUNT_DIR"
ls "$MOUNT_DIR" | grep -q "mount-v06.txt"
grep -q "hello from v0.6 mount" "$MOUNT_DIR/mount-v06.txt"

cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml fs write node-a:/mount-v06-live.txt --content "live update from remote"
grep -q "live update from remote" "$MOUNT_DIR/mount-v06-live.txt"

if cargo run -q -p operon-cli -- --config examples/docker-nodes.yaml mount node-a:/../secret --to "$TMP_DIR/bad-mount" >"$TMP_DIR/bad-mount.log" 2>&1; then
  echo "expected path escape mount to fail" >&2
  exit 1
fi
cat "$TMP_DIR/bad-mount.log"

kill -INT "$MOUNT_PID"
wait "$MOUNT_PID"
MOUNT_PID=""
if mountpoint -q "$MOUNT_DIR"; then
  echo "expected mount to be unmounted after Ctrl-C" >&2
  exit 1
fi

mkdir -p "$DENY_WORKSPACE"
cat >"$DENY_POLICY" <<'YAML'
subject: v06-deny

fs:
  mounts:
    - name: workspace
      path: /
      permissions:
        read: false
        write: false
        delete: false

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

cat >"$DENY_NODES" <<'YAML'
nodes:
  deny:
    endpoint: grpc://127.0.0.1:18789
    token: deny-token
YAML

cargo run -q -p operond -- start \
  --grpc-listen 127.0.0.1:18789 \
  --node-id deny \
  --workspace "$DENY_WORKSPACE" \
  --policy "$DENY_POLICY" \
  --auth-token deny-token \
  --store "$DENY_STORE" >"$TMP_DIR/deny-daemon.log" 2>&1 &
DENY_PID="$!"

for _ in $(seq 1 30); do
  if cargo run -q -p operon-cli -- --config "$DENY_NODES" node ping deny >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
cargo run -q -p operon-cli -- --config "$DENY_NODES" node ping deny

if cargo run -q -p operon-cli -- --config "$DENY_NODES" mount deny:/ --to "$TMP_DIR/deny-mount" >"$DENY_LOG" 2>&1; then
  echo "expected policy-denied mount to fail" >&2
  exit 1
fi
cat "$DENY_LOG"

cargo run -q -p operon-cli -- --config "$DENY_NODES" audit show deny --capability fs:workspace --action stat --allowed false --resource / --limit 5 >"$TMP_DIR/deny-audit.log"
cat "$TMP_DIR/deny-audit.log"
grep -Eq "fs:workspace[[:space:]]+stat[[:space:]]+/[[:space:]]+false" "$TMP_DIR/deny-audit.log"

echo "v0.6 Linux mount read validation passed"
