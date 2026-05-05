#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "v0.14 macOS fuser hello probe requires macOS" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
PROBE_DIR="$TMP_DIR/fuser-hello"
MOUNT_NAME="operon-v014-fuser-hello-$$"
MOUNT_DIR="/Volumes/$MOUNT_NAME"
PROBE_LOG="$TMP_DIR/fuser-hello.log"
PROBE_PID=""
WATCHDOG_PID=""
SMOKE_TIMEOUT_SECS="${OPERON_SMOKE_TIMEOUT_SECS:-300}"
PATCH_INIT_FLAGS="${OPERON_FUSER_HELLO_PATCH_INIT_FLAGS:-0}"
export DYLD_LIBRARY_PATH="/usr/local/lib:/opt/homebrew/lib:${DYLD_LIBRARY_PATH:-}"

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

dump_diagnostics() {
  (
    set +e
    echo "temporary probe directory: $TMP_DIR" >&2
    echo "mount directory: $MOUNT_DIR" >&2
    echo "macOS mount backend: ${OPERON_MOUNT_MACOS_BACKEND:-<unset>}" >&2
    echo "macOS mount extra options: ${OPERON_MOUNT_MACOS_OPTIONS:-<none>}" >&2
    echo "patched fuser init flags: $PATCH_INIT_FLAGS" >&2
    sw_vers >&2 || true
    pkg-config --modversion fuse >&2 || true
    pkg-config --libs fuse >&2 || true
    pkg-config --cflags fuse >&2 || true
    ps -axo pid,ppid,stat,command | grep -Ei 'fuse-t|nfsd|mount_nfs|mount_smbfs|fuser_hello' | grep -v grep >&2 || true
    nfsd status >&2 || true
    echo "=== macOS unified FUSE/NFS logs ===" >&2
    log show --last 3m --style compact --predicate 'process CONTAINS[c] "fuse-t" OR process CONTAINS[c] "nfsd" OR process CONTAINS[c] "mount_nfs" OR eventMessage CONTAINS[c] "fuse-t" OR eventMessage CONTAINS[c] "mount_nfs" OR eventMessage CONTAINS[c] "NFS" OR eventMessage CONTAINS[c] "nfs"' >&2 || true
    echo "=== FUSE-T user logs ===" >&2
    find "$HOME/Library/Logs/fuse-t" -maxdepth 2 -type f -print -exec tail -200 {} \; >&2 || true
    mount >&2 || true
    echo "=== fuser hello log ===" >&2
    cat "$PROBE_LOG" >&2 || true
    echo "=== temp files ===" >&2
    find "$TMP_DIR" -maxdepth 3 -print >&2 || true
  )
}

cleanup() {
  set +e
  if [[ -n "$WATCHDOG_PID" ]] && kill -0 "$WATCHDOG_PID" >/dev/null 2>&1; then
    kill "$WATCHDOG_PID" >/dev/null 2>&1 || true
    wait_for_process_exit "$WATCHDOG_PID" 2 || true
  fi
  if [[ -n "$PROBE_PID" ]] && kill -0 "$PROBE_PID" >/dev/null 2>&1; then
    kill -INT "$PROBE_PID" >/dev/null 2>&1 || true
    wait_for_process_exit "$PROBE_PID" 5 || true
    if kill -0 "$PROBE_PID" >/dev/null 2>&1; then
      kill -KILL "$PROBE_PID" >/dev/null 2>&1 || true
      wait_for_process_exit "$PROBE_PID" 2 || true
    fi
  fi
  if mount | grep -F " on $MOUNT_DIR " >/dev/null 2>&1; then
    umount "$MOUNT_DIR" >/dev/null 2>&1 || true
  fi
  sudo rmdir "$MOUNT_DIR" >/dev/null 2>&1 || true
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT
trap 'dump_diagnostics; exit 124' TERM

start_watchdog() {
  perl -e 'my ($timeout, $pid) = @ARGV; sleep $timeout; print STDERR "macOS fuser hello probe timed out after ${timeout}s\n"; kill "TERM", $pid;' "$SMOKE_TIMEOUT_SECS" "$$" &
  WATCHDOG_PID="$!"
}

write_probe_project() {
  mkdir -p "$PROBE_DIR/src"
  cat >"$PROBE_DIR/Cargo.toml" <<'TOML'
[package]
name = "operon-v014-fuser-hello-probe"
version = "0.0.0"
edition = "2021"

[[bin]]
name = "fuser_hello"
path = "src/main.rs"

[dependencies]
anyhow = "1"
env_logger = "0.11"
fuser = "0.17.0"
libc = "0.2"
TOML

  cat >"$PROBE_DIR/src/main.rs" <<'RS'
use std::{
    env,
    ffi::OsStr,
    path::PathBuf,
    thread,
    time::{Duration, UNIX_EPOCH},
};

use fuser::{
    Errno, FileAttr, FileHandle, FileType, Filesystem, INodeNo, OpenFlags, ReplyAttr, ReplyData,
    ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, Request,
};

const TTL: Duration = Duration::from_secs(1);
const CONTENT: &[u8] = b"hello from fuser\n";

fn trace(event: &str, detail: impl AsRef<str>) {
    eprintln!("fuser-hello {event}: {}", detail.as_ref());
}

fn owner() -> (u32, u32) {
    unsafe { (libc::getuid(), libc::getgid()) }
}

fn attr(ino: INodeNo) -> Option<FileAttr> {
    let (uid, gid) = owner();
    match ino.0 {
        1 => Some(FileAttr {
            ino,
            size: 0,
            blocks: 0,
            atime: UNIX_EPOCH,
            mtime: UNIX_EPOCH,
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: FileType::Directory,
            perm: 0o755,
            nlink: 2,
            uid,
            gid,
            rdev: 0,
            flags: 0,
            blksize: 512,
        }),
        2 => Some(FileAttr {
            ino,
            size: CONTENT.len() as u64,
            blocks: 1,
            atime: UNIX_EPOCH,
            mtime: UNIX_EPOCH,
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: FileType::RegularFile,
            perm: 0o644,
            nlink: 1,
            uid,
            gid,
            rdev: 0,
            flags: 0,
            blksize: 512,
        }),
        _ => None,
    }
}

struct HelloFs;

impl Filesystem for HelloFs {
    fn init(&mut self, _req: &Request, _config: &mut fuser::KernelConfig) -> std::io::Result<()> {
        trace("init", "ok");
        Ok(())
    }

    fn destroy(&mut self) {
        trace("destroy", "ok");
    }

    fn statfs(&self, _req: &Request, ino: INodeNo, reply: ReplyStatfs) {
        trace("statfs", format!("ino={ino:?}"));
        reply.statfs(1_048_576, 1_048_576, 1_048_576, 2, 0, 1, 255, 1);
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        trace("getattr", format!("ino={ino:?}"));
        match attr(ino) {
            Some(attr) => {
                trace(
                    "getattr_attr",
                    format!(
                        "ino={:?} kind={:?} size={} blocks={} blksize={} uid={} gid={}",
                        attr.ino, attr.kind, attr.size, attr.blocks, attr.blksize, attr.uid, attr.gid
                    ),
                );
                reply.attr(&TTL, &attr);
            }
            None => reply.error(Errno::ENOENT),
        }
    }

    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        trace("lookup", format!("parent={parent:?} name={name:?}"));
        if parent.0 == 1 && name == OsStr::new("hello.txt") {
            let attr = attr(INodeNo(2)).expect("hello attr");
            reply.entry(&TTL, &attr, fuser::Generation(0));
        } else {
            reply.error(Errno::ENOENT);
        }
    }

    fn opendir(&self, _req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        trace("opendir", format!("ino={ino:?}"));
        if ino.0 == 1 {
            reply.opened(FileHandle(1), fuser::FopenFlags::empty());
        } else {
            reply.error(Errno::ENOTDIR);
        }
    }

    fn releasedir(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        _flags: OpenFlags,
        reply: ReplyEmpty,
    ) {
        trace("releasedir", format!("ino={ino:?}"));
        reply.ok();
    }

    fn readdir(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        trace("readdir", format!("ino={ino:?} offset={offset}"));
        if ino.0 != 1 {
            reply.error(Errno::ENOTDIR);
            return;
        }
        let entries = [
            (INodeNo(1), FileType::Directory, "."),
            (INodeNo(1), FileType::Directory, ".."),
            (INodeNo(2), FileType::RegularFile, "hello.txt"),
        ];
        for (index, (ino, kind, name)) in entries.into_iter().enumerate().skip(offset as usize) {
            if reply.add(ino, (index + 1) as u64, kind, name) {
                break;
            }
        }
        reply.ok();
    }

    fn access(&self, _req: &Request, ino: INodeNo, mask: fuser::AccessFlags, reply: ReplyEmpty) {
        trace("access", format!("ino={ino:?} mask={mask}"));
        if attr(ino).is_some() {
            reply.ok();
        } else {
            reply.error(Errno::ENOENT);
        }
    }

    fn open(&self, _req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        trace("open", format!("ino={ino:?}"));
        if ino.0 == 2 {
            reply.opened(FileHandle(2), fuser::FopenFlags::empty());
        } else {
            reply.error(Errno::ENOENT);
        }
    }

    fn read(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        reply: ReplyData,
    ) {
        trace("read", format!("ino={ino:?} offset={offset} size={size}"));
        if ino.0 != 2 {
            reply.error(Errno::ENOENT);
            return;
        }
        let start = (offset as usize).min(CONTENT.len());
        let end = (start + size as usize).min(CONTENT.len());
        reply.data(&CONTENT[start..end]);
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("fuser=warn"))
        .format_timestamp_millis()
        .init();
    let mount_point = PathBuf::from(env::args().nth(1).expect("mount point argument"));
    let backend = env::var("OPERON_MOUNT_MACOS_BACKEND").unwrap_or_else(|_| "nfs".to_string());
    let mut config = fuser::Config::default();
    config
        .mount_options
        .push(fuser::MountOption::CUSTOM(format!("backend={backend}")));
    if let Ok(extra) = env::var("OPERON_MOUNT_MACOS_OPTIONS") {
        for option in extra.split(',').map(str::trim).filter(|option| !option.is_empty()) {
            config
                .mount_options
                .push(fuser::MountOption::CUSTOM(option.to_string()));
        }
    }
    config.n_threads = Some(1);
    trace("spawn_mount2_start", mount_point.display().to_string());
    let _session = fuser::spawn_mount2(HelloFs, &mount_point, &config)?;
    trace("spawn_mount2_ok", mount_point.display().to_string());
    loop {
        thread::park_timeout(Duration::from_secs(60));
    }
}
RS
}

patch_fuser_init_flags() {
  if [[ "$PATCH_INIT_FLAGS" != "1" ]]; then
    return 0
  fi

  local patched_dir="$ROOT_DIR/vendor/fuser-0.17.0-operon"
  if [[ ! -d "$patched_dir" ]]; then
    echo "failed to locate Operon-patched fuser source at $patched_dir" >&2
    exit 1
  fi

  cat >>"$PROBE_DIR/Cargo.toml" <<TOML

[patch.crates-io]
fuser = { path = "$patched_dir" }
TOML
}

wait_for_probe_mount() {
  for _ in $(seq 1 30); do
    if [[ -n "$PROBE_PID" ]] && ! kill -0 "$PROBE_PID" >/dev/null 2>&1; then
      echo "fuser hello process exited before exposing hello.txt" >&2
      dump_diagnostics
      return 1
    fi
    if [[ -f "$MOUNT_DIR/hello.txt" ]]; then
      return 0
    fi
    sleep 1
  done
  echo "fuser hello did not expose hello.txt" >&2
  dump_diagnostics
  return 1
}

echo "macOS fuser hello backend: ${OPERON_MOUNT_MACOS_BACKEND:-nfs}" >&2
echo "macOS fuser hello extra options: ${OPERON_MOUNT_MACOS_OPTIONS:-<none>}" >&2
echo "patched fuser init flags: $PATCH_INIT_FLAGS" >&2
if ! pkg-config --modversion fuse >/dev/null 2>&1; then
  echo "pkg-config cannot resolve fuse; install FUSE-T before running fuser hello probe" >&2
  exit 1
fi

start_watchdog
write_probe_project
patch_fuser_init_flags
sudo mkdir -p "$MOUNT_DIR"
sudo chown "$(id -u):$(id -g)" "$MOUNT_DIR"
cargo build -q --manifest-path "$PROBE_DIR/Cargo.toml"

"$PROBE_DIR/target/debug/fuser_hello" "$MOUNT_DIR" >"$PROBE_LOG" 2>&1 &
PROBE_PID="$!"
wait_for_probe_mount
grep -q "^hello from fuser$" "$MOUNT_DIR/hello.txt"

echo "v0.14 macOS FUSE-T fuser hello probe passed"
