#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "FUSE-T fuse-zip probe requires macOS" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
SRC_DIR="$TMP_DIR/fuse-zip"
ZIP_ROOT="$TMP_DIR/zip-root"
ZIP_PATH="$TMP_DIR/seed.zip"
MOUNT_NAME="operon-live-mount-fuse-zip-probe-$$"
MOUNT_DIR="/Volumes/$MOUNT_NAME"
FUSE_ZIP_LOG="$TMP_DIR/fuse-zip.log"
FUSE_ZIP_PID=""
WATCHDOG_PID=""
SMOKE_TIMEOUT_SECS="${OPERON_SMOKE_TIMEOUT_SECS:-180}"
export DYLD_LIBRARY_PATH="/usr/local/lib:/opt/homebrew/lib:${DYLD_LIBRARY_PATH:-}"
export PKG_CONFIG_PATH="/usr/local/lib/pkgconfig:/opt/homebrew/lib/pkgconfig:/Library/Application Support/fuse-t/pkgconfig:${PKG_CONFIG_PATH:-}"

dump_diagnostics() {
  (
    set +e
    echo "temporary probe directory: $TMP_DIR" >&2
    echo "mount directory: $MOUNT_DIR" >&2
    echo "macOS mount backend: ${OPERON_MOUNT_MACOS_BACKEND:-<unset>}" >&2
    echo "macOS mount extra options: ${OPERON_MOUNT_MACOS_OPTIONS:-<none>}" >&2
    sw_vers >&2 || true
    uname -a >&2 || true
    pkg-config --modversion fuse >&2 || true
    pkg-config --libs fuse >&2 || true
    pkg-config --cflags fuse >&2 || true
    pkg-config --modversion libzip >&2 || true
    pkg-config --libs libzip >&2 || true
    ps -axo pid,ppid,stat,command | grep -Ei 'fuse-zip|fuse-t|nfsd|mount_nfs|mount_smbfs' | grep -v grep >&2 || true
    sudo lsof -nP -iTCP -iUDP | grep -Ei 'fuse|nfs|smb|mount' >&2 || true
    log show --last 3m --style compact --predicate 'process CONTAINS[c] "fuse-t" OR process CONTAINS[c] "fuse-zip" OR process CONTAINS[c] "nfsd" OR eventMessage CONTAINS[c] "fuse-t"' >&2 || true
    echo "=== FUSE-T user logs ===" >&2
    find "$HOME/Library/Logs/fuse-t" -maxdepth 2 -type f -print -exec tail -200 {} \; >&2 || true
    echo "=== fuse-zip log ===" >&2
    cat "$FUSE_ZIP_LOG" >&2 || true
    echo "=== temp files ===" >&2
    find "$TMP_DIR" -maxdepth 3 -print >&2 || true
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

run_with_timeout() {
  local attempts="$1"
  shift
  "$@" &
  local pid="$!"
  if wait_for_process_exit "$pid" "$attempts"; then
    return 0
  fi
  kill -TERM "$pid" >/dev/null 2>&1 || true
  wait_for_process_exit "$pid" 2 || true
  if kill -0 "$pid" >/dev/null 2>&1; then
    kill -KILL "$pid" >/dev/null 2>&1 || true
    wait_for_process_exit "$pid" 2 || true
  fi
  return 124
}

cleanup() {
  set +e
  if [[ -n "$WATCHDOG_PID" ]] && kill -0 "$WATCHDOG_PID" >/dev/null 2>&1; then
    kill "$WATCHDOG_PID" >/dev/null 2>&1 || true
    wait_for_process_exit "$WATCHDOG_PID" 2 || true
  fi
  if [[ -n "$FUSE_ZIP_PID" ]] && kill -0 "$FUSE_ZIP_PID" >/dev/null 2>&1; then
    kill -INT "$FUSE_ZIP_PID" >/dev/null 2>&1 || true
    wait_for_process_exit "$FUSE_ZIP_PID" 5 || true
    if kill -0 "$FUSE_ZIP_PID" >/dev/null 2>&1; then
      kill -TERM "$FUSE_ZIP_PID" >/dev/null 2>&1 || true
      wait_for_process_exit "$FUSE_ZIP_PID" 2 || true
    fi
    if kill -0 "$FUSE_ZIP_PID" >/dev/null 2>&1; then
      kill -KILL "$FUSE_ZIP_PID" >/dev/null 2>&1 || true
      wait_for_process_exit "$FUSE_ZIP_PID" 2 || true
    fi
  fi
  if mount | grep -F " on $MOUNT_DIR " >/dev/null 2>&1; then
    run_with_timeout 5 umount "$MOUNT_DIR" >/dev/null 2>&1 || true
  fi
  run_with_timeout 5 sudo rmdir "$MOUNT_DIR" >/dev/null 2>&1 || true
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT
trap 'dump_diagnostics; exit 124' TERM

start_watchdog() {
  perl -e 'my ($timeout, $pid) = @ARGV; sleep $timeout; print STDERR "macOS FUSE-T fuse-zip probe timed out after ${timeout}s\n"; kill "TERM", $pid;' "$SMOKE_TIMEOUT_SECS" "$$" &
  WATCHDOG_PID="$!"
}

ensure_runtime() {
  export OPERON_MOUNT_MACOS_BACKEND="${OPERON_MOUNT_MACOS_BACKEND:-nfs}"
  echo "macOS mount backend: $OPERON_MOUNT_MACOS_BACKEND" >&2
  echo "macOS mount extra options: ${OPERON_MOUNT_MACOS_OPTIONS:-<none>}" >&2
  if [[ "$OPERON_MOUNT_MACOS_BACKEND" != "nfs" && "$OPERON_MOUNT_MACOS_BACKEND" != "smb" && "$OPERON_MOUNT_MACOS_BACKEND" != "fskit" ]]; then
    echo "unsupported OPERON_MOUNT_MACOS_BACKEND: $OPERON_MOUNT_MACOS_BACKEND" >&2
    echo "expected nfs, smb, or fskit" >&2
    exit 1
  fi
  if ! pkg-config --modversion fuse >/dev/null 2>&1; then
    echo "pkg-config cannot resolve fuse; install FUSE-T before running fuse-zip probe" >&2
    exit 1
  fi
  if ! pkg-config --modversion libzip >/dev/null 2>&1; then
    echo "pkg-config cannot resolve libzip; install libzip before running fuse-zip probe" >&2
    exit 1
  fi
}

wait_for_mount() {
  for _ in $(seq 1 30); do
    if [[ -n "$FUSE_ZIP_PID" ]] && ! kill -0 "$FUSE_ZIP_PID" >/dev/null 2>&1; then
      echo "fuse-zip process exited before exposing seed file" >&2
      dump_diagnostics
      return 1
    fi
    if [[ -f "$MOUNT_DIR/seed.txt" ]]; then
      return 0
    fi
    sleep 1
  done
  echo "fuse-zip did not expose seed file" >&2
  dump_diagnostics
  return 1
}

start_watchdog
ensure_runtime

git clone --depth 1 https://github.com/macos-fuse-t/fuse-zip "$SRC_DIR"
FUSE_ZIP_CXXFLAGS="-O2 -Wall -Wextra -Wconversion -Wsign-conversion -Wshadow -pedantic -std=c++11 $(pkg-config --cflags fuse) $(pkg-config --cflags libzip)"
FUSE_ZIP_LIBS="-Llib -lfusezip $(pkg-config --libs fuse) $(pkg-config --libs libzip)"
make -C "$SRC_DIR" CXXFLAGS="$FUSE_ZIP_CXXFLAGS" LIBS="$FUSE_ZIP_LIBS" all doc

mkdir -p "$ZIP_ROOT"
printf "seed" >"$ZIP_ROOT/seed.txt"
(cd "$ZIP_ROOT" && zip -qr "$ZIP_PATH" seed.txt)
sudo mkdir -p "$MOUNT_DIR"
sudo chown "$(id -u):$(id -g)" "$MOUNT_DIR"

mount_options="backend=${OPERON_MOUNT_MACOS_BACKEND}"
if [[ -n "${OPERON_MOUNT_MACOS_OPTIONS:-}" ]]; then
  mount_options="${mount_options},${OPERON_MOUNT_MACOS_OPTIONS}"
fi

"$SRC_DIR/fuse-zip" -f -o "$mount_options" "$ZIP_PATH" "$MOUNT_DIR" >"$FUSE_ZIP_LOG" 2>&1 &
FUSE_ZIP_PID="$!"
echo "fuse-zip started: pid=$FUSE_ZIP_PID mount=$MOUNT_DIR"
echo "waiting for fuse-zip seed file"
wait_for_mount

grep -q "^seed$" "$MOUNT_DIR/seed.txt"
echo "fuse-zip exposed seed file through FUSE-T"
echo "unmounting fuse-zip mount"
run_with_timeout 5 umount "$MOUNT_DIR"
echo "waiting for fuse-zip process exit"
if wait_for_process_exit "$FUSE_ZIP_PID" 10; then
  FUSE_ZIP_PID=""
else
  echo "fuse-zip process did not exit after unmount" >&2
  dump_diagnostics
  exit 1
fi

echo "macOS FUSE-T fuse-zip probe passed"
