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
MOUNT_NAME="operon-v014-macos-live-mount-$$"
MOUNT_DIR="/Volumes/$MOUNT_NAME"
MOUNT_LOG="$TMP_DIR/mount.log"
DAEMON_LOG="$TMP_DIR/daemon.log"
DAEMON_PID=""
MOUNT_PID=""
WATCHDOG_PID=""
SMOKE_TIMEOUT_SECS="${OPERON_SMOKE_TIMEOUT_SECS:-600}"
OPEROND_BIN="$ROOT_DIR/target/debug/operond"
OPERON_BIN="$ROOT_DIR/target/debug/operon"
export DYLD_LIBRARY_PATH="/usr/local/lib:/opt/homebrew/lib:${DYLD_LIBRARY_PATH:-}"

dump_diagnostics() {
  (
    set +e
    echo "temporary smoke directory: $TMP_DIR" >&2
    echo "mount directory: $MOUNT_DIR" >&2
    sw_vers >&2 || true
    uname -a >&2 || true
    pkg-config --modversion fuse >&2 || true
    pkg-config --libs fuse >&2 || true
    pkg-config --cflags fuse >&2 || true
    pkg-config --libs fuse-t >&2 || true
    pkg-config --cflags fuse-t >&2 || true
    cat /usr/local/lib/pkgconfig/fuse.pc >&2 || true
    cat /usr/local/lib/pkgconfig/fuse-t.pc >&2 || true
    ls -la "/Library/Application Support/fuse-t" >&2 || true
    find /usr/local/lib /opt/homebrew/lib -maxdepth 2 \( -iname '*fuse*' -o -iname '*nfs*' \) -print >&2 || true
    ps -axo pid,ppid,stat,command | grep -Ei 'fuse-t|nfsd|mount_nfs|mount_smbfs' | grep -v grep >&2 || true
    nfsd status >&2 || true
    sudo lsof -nP -iTCP -iUDP | grep -Ei 'fuse|nfs|smb|mount' >&2 || true
    log show --last 3m --style compact --predicate 'process CONTAINS[c] "fuse-t" OR process CONTAINS[c] "nfsd" OR eventMessage CONTAINS[c] "fuse-t"' >&2 || true
    if [[ -n "$DAEMON_PID" ]]; then
      ps -p "$DAEMON_PID" -o pid,stat,command >&2 || true
    fi
    if [[ -n "$MOUNT_PID" ]]; then
      ps -p "$MOUNT_PID" -o pid,stat,command >&2 || true
    fi
    mount >&2 || true
    echo "=== daemon log ===" >&2
    cat "$DAEMON_LOG" >&2 || true
    echo "=== mount log ===" >&2
    cat "$MOUNT_LOG" >&2 || true
    echo "=== temp files ===" >&2
    find "$TMP_DIR" -maxdepth 2 -print >&2 || true
  )
}

wait_for_process_exit() {
  local pid="$1"
  local attempts="$2"
  for _ in $(seq 1 "$attempts"); do
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      wait "$pid" >/dev/null 2>&1 || true
      return 0
    fi
    sleep 1
  done
  return 1
}

cleanup() {
  set +e
  if [[ -n "$WATCHDOG_PID" ]] && kill -0 "$WATCHDOG_PID" >/dev/null 2>&1; then
    kill "$WATCHDOG_PID" >/dev/null 2>&1 || true
    wait_for_process_exit "$WATCHDOG_PID" 2 || true
  fi
  if [[ -n "$MOUNT_PID" ]] && kill -0 "$MOUNT_PID" >/dev/null 2>&1; then
    kill -INT "$MOUNT_PID" >/dev/null 2>&1
    wait_for_process_exit "$MOUNT_PID" 5 || true
    if kill -0 "$MOUNT_PID" >/dev/null 2>&1; then
      kill -TERM "$MOUNT_PID" >/dev/null 2>&1 || true
      wait_for_process_exit "$MOUNT_PID" 2 || true
    fi
    if kill -0 "$MOUNT_PID" >/dev/null 2>&1; then
      kill -KILL "$MOUNT_PID" >/dev/null 2>&1 || true
      wait_for_process_exit "$MOUNT_PID" 2 || true
    fi
    if kill -0 "$MOUNT_PID" >/dev/null 2>&1; then
      echo "mount process $MOUNT_PID did not exit after SIGKILL; leaving runner cleanup to reap it" >&2
    fi
  fi
  if mount | grep -F " on $MOUNT_DIR " >/dev/null 2>&1; then
    umount "$MOUNT_DIR" >/dev/null 2>&1 || true
  fi
  sudo rmdir "$MOUNT_DIR" >/dev/null 2>&1 || true
  if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
    kill "$DAEMON_PID" >/dev/null 2>&1
    wait_for_process_exit "$DAEMON_PID" 5 || true
    if kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
      kill -KILL "$DAEMON_PID" >/dev/null 2>&1 || true
      wait_for_process_exit "$DAEMON_PID" 2 || true
    fi
    if kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
      echo "daemon process $DAEMON_PID did not exit after SIGKILL; leaving runner cleanup to reap it" >&2
    fi
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT
trap 'dump_diagnostics; exit 124' TERM

start_watchdog() {
  (
    sleep "$SMOKE_TIMEOUT_SECS"
    echo "macOS live mount smoke timed out after ${SMOKE_TIMEOUT_SECS}s" >&2
    kill -TERM "$$" >/dev/null 2>&1 || true
  ) &
  WATCHDOG_PID="$!"
}

ensure_fuse_t_runtime() {
  export OPERON_MOUNT_MACOS_BACKEND="${OPERON_MOUNT_MACOS_BACKEND:-nfs}"
  echo "macOS mount backend: $OPERON_MOUNT_MACOS_BACKEND" >&2
  if [[ "$OPERON_MOUNT_MACOS_BACKEND" != "nfs" && "$OPERON_MOUNT_MACOS_BACKEND" != "smb" && "$OPERON_MOUNT_MACOS_BACKEND" != "fskit" ]]; then
    echo "unsupported OPERON_MOUNT_MACOS_BACKEND: $OPERON_MOUNT_MACOS_BACKEND" >&2
    echo "expected nfs, smb, or fskit" >&2
    exit 1
  fi

  if ! pkg-config --modversion fuse >/dev/null 2>&1; then
    echo "pkg-config cannot resolve fuse; install FUSE-T before running macOS live mount smoke" >&2
    exit 1
  fi
}

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
    if "$OPERON_BIN" --config "$CONFIG" node ping macos-live >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  "$OPERON_BIN" --config "$CONFIG" node ping macos-live
}

wait_for_mount() {
  for _ in $(seq 1 30); do
    if [[ -n "$MOUNT_PID" ]] && ! kill -0 "$MOUNT_PID" >/dev/null 2>&1; then
      echo "mount process exited before exposing seed file" >&2
      dump_diagnostics
      return 1
    fi
    if [[ -f "$MOUNT_DIR/seed.txt" ]]; then
      return 0
    fi
    sleep 1
  done
  echo "mount did not expose seed file" >&2
  dump_diagnostics
  return 1
}

mkdir -p "$WORKSPACE"
sudo mkdir -p "$MOUNT_DIR"
sudo chown "$(id -u):$(id -g)" "$MOUNT_DIR"
printf "seed" >"$WORKSPACE/seed.txt"
write_config
start_watchdog
ensure_fuse_t_runtime

cargo build -q -p operond -p operon-cli --locked

"$OPEROND_BIN" start --config "$CONFIG" >"$DAEMON_LOG" 2>&1 &
DAEMON_PID="$!"
wait_for_node

OPERON_MOUNT_TRACE=1 "$OPERON_BIN" --config "$CONFIG" mount macos-live:/ --to "$MOUNT_DIR" >"$MOUNT_LOG" 2>&1 &
MOUNT_PID="$!"
wait_for_mount

grep -q "^seed$" "$MOUNT_DIR/seed.txt"

printf "created through macos mount" >"$MOUNT_DIR/new.txt"
"$OPERON_BIN" --config "$CONFIG" fs read macos-live:/new.txt >"$TMP_DIR/new-read.txt"
grep -q "^created through macos mount$" "$TMP_DIR/new-read.txt"

mkdir "$MOUNT_DIR/dir"
printf "abcdef" >"$MOUNT_DIR/dir/data.txt"
truncate -s 3 "$MOUNT_DIR/dir/data.txt"
grep -q "^abc$" "$MOUNT_DIR/dir/data.txt"
mv "$MOUNT_DIR/dir/data.txt" "$MOUNT_DIR/dir/renamed.txt"
"$OPERON_BIN" --config "$CONFIG" fs read macos-live:/dir/renamed.txt >"$TMP_DIR/renamed-read.txt"
grep -q "^abc$" "$TMP_DIR/renamed-read.txt"
rm "$MOUNT_DIR/dir/renamed.txt"
rmdir "$MOUNT_DIR/dir"

echo "v0.14 macOS live mount smoke passed"
