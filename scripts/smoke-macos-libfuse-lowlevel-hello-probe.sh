#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS libfuse low-level hello probe requires macOS" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
SRC_PATH="$TMP_DIR/hello_ll.c"
BIN_PATH="$TMP_DIR/hello_ll"
MOUNT_NAME="operon-libfuse-ll-hello-$$"
MOUNT_DIR="/Volumes/$MOUNT_NAME"
PROBE_LOG="$TMP_DIR/libfuse-lowlevel-hello.log"
PROBE_PID=""
WATCHDOG_PID=""
SMOKE_TIMEOUT_SECS="${OPERON_SMOKE_TIMEOUT_SECS:-180}"
export DYLD_LIBRARY_PATH="/usr/local/lib:/opt/homebrew/lib:${DYLD_LIBRARY_PATH:-}"
export PKG_CONFIG_PATH="/usr/local/lib/pkgconfig:/opt/homebrew/lib/pkgconfig:/Library/Application Support/fuse-t/pkgconfig:${PKG_CONFIG_PATH:-}"

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
    sw_vers >&2 || true
    pkg-config --modversion fuse >&2 || true
    pkg-config --libs fuse >&2 || true
    pkg-config --cflags fuse >&2 || true
    ps -axo pid,ppid,stat,command | grep -Ei 'hello_ll|fuse-t|nfsd|mount_nfs|mount_smbfs' | grep -v grep >&2 || true
    nfsd status >&2 || true
    echo "=== FUSE-T user logs ===" >&2
    find "$HOME/Library/Logs/fuse-t" -maxdepth 2 -type f -print -exec tail -200 {} \; >&2 || true
    mount >&2 || true
    echo "=== libfuse low-level hello log ===" >&2
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
  perl -e 'my ($timeout, $pid) = @ARGV; sleep $timeout; print STDERR "macOS libfuse low-level hello probe timed out after ${timeout}s\n"; kill "TERM", $pid;' "$SMOKE_TIMEOUT_SECS" "$$" &
  WATCHDOG_PID="$!"
}

write_probe_source() {
  cat >"$SRC_PATH" <<'C'
#define FUSE_USE_VERSION 26

#include <errno.h>
#include <fcntl.h>
#include <fuse_lowlevel.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/statvfs.h>
#include <unistd.h>

static const char *hello_name = "hello.txt";
static const char *hello_body = "hello from libfuse lowlevel\n";

static void trace(const char *event) {
    fprintf(stderr, "libfuse-ll-hello %s\n", event);
    fflush(stderr);
}

static int fill_stat(fuse_ino_t ino, struct stat *st) {
    memset(st, 0, sizeof(*st));
    st->st_ino = ino;
    st->st_uid = getuid();
    st->st_gid = getgid();
    st->st_blksize = 512;
    if (ino == 1) {
        st->st_mode = S_IFDIR | 0755;
        st->st_nlink = 2;
        st->st_blocks = 0;
        return 0;
    }
    if (ino == 2) {
        st->st_mode = S_IFREG | 0644;
        st->st_nlink = 1;
        st->st_size = (off_t)strlen(hello_body);
        st->st_blocks = 1;
        return 0;
    }
    return -ENOENT;
}

static void hello_init(void *userdata, struct fuse_conn_info *conn) {
    (void)userdata;
    fprintf(stderr, "libfuse-ll-hello init proto=%u.%u max_write=%u capable=0x%08x want=0x%08x\n",
            conn->proto_major, conn->proto_minor, conn->max_write, conn->capable, conn->want);
    fflush(stderr);
}

static void hello_destroy(void *userdata) {
    (void)userdata;
    trace("destroy");
}

static void hello_statfs(fuse_req_t req, fuse_ino_t ino) {
    struct statvfs st;
    fprintf(stderr, "libfuse-ll-hello statfs ino=%llu\n", (unsigned long long)ino);
    fflush(stderr);
    memset(&st, 0, sizeof(st));
    st.f_bsize = 1;
    st.f_frsize = 1;
    st.f_blocks = 1048576;
    st.f_bfree = 1048576;
    st.f_bavail = 1048576;
    st.f_files = 2;
    st.f_ffree = 0;
    st.f_namemax = 255;
    fuse_reply_statfs(req, &st);
}

static void hello_getattr(fuse_req_t req, fuse_ino_t ino, struct fuse_file_info *fi) {
    struct stat st;
    (void)fi;
    fprintf(stderr, "libfuse-ll-hello getattr ino=%llu\n", (unsigned long long)ino);
    fflush(stderr);
    if (fill_stat(ino, &st) == 0) {
        fuse_reply_attr(req, &st, 1.0);
    } else {
        fuse_reply_err(req, ENOENT);
    }
}

static void hello_lookup(fuse_req_t req, fuse_ino_t parent, const char *name) {
    struct fuse_entry_param entry;
    fprintf(stderr, "libfuse-ll-hello lookup parent=%llu name=%s\n", (unsigned long long)parent, name);
    fflush(stderr);
    if (parent != 1 || strcmp(name, hello_name) != 0) {
        fuse_reply_err(req, ENOENT);
        return;
    }
    memset(&entry, 0, sizeof(entry));
    entry.ino = 2;
    entry.attr_timeout = 1.0;
    entry.entry_timeout = 1.0;
    fill_stat(2, &entry.attr);
    fuse_reply_entry(req, &entry);
}

struct dirbuf {
    char *data;
    size_t size;
};

static void dirbuf_add(fuse_req_t req, struct dirbuf *buf, const char *name, fuse_ino_t ino) {
    struct stat st;
    size_t old_size = buf->size;
    buf->size += fuse_add_direntry(req, NULL, 0, name, NULL, 0);
    buf->data = realloc(buf->data, buf->size);
    fill_stat(ino, &st);
    fuse_add_direntry(req, buf->data + old_size, buf->size - old_size, name, &st, buf->size);
}

static void reply_limited(fuse_req_t req, const char *data, size_t data_size, off_t off, size_t size) {
    if ((size_t)off < data_size) {
        size_t remaining = data_size - (size_t)off;
        fuse_reply_buf(req, data + off, remaining < size ? remaining : size);
    } else {
        fuse_reply_buf(req, NULL, 0);
    }
}

static void hello_readdir(fuse_req_t req, fuse_ino_t ino, size_t size, off_t off, struct fuse_file_info *fi) {
    struct dirbuf buf = {0};
    (void)fi;
    fprintf(stderr, "libfuse-ll-hello readdir ino=%llu off=%lld size=%zu\n", (unsigned long long)ino, (long long)off, size);
    fflush(stderr);
    if (ino != 1) {
        fuse_reply_err(req, ENOTDIR);
        return;
    }
    dirbuf_add(req, &buf, ".", 1);
    dirbuf_add(req, &buf, "..", 1);
    dirbuf_add(req, &buf, hello_name, 2);
    reply_limited(req, buf.data, buf.size, off, size);
    free(buf.data);
}

static void hello_open(fuse_req_t req, fuse_ino_t ino, struct fuse_file_info *fi) {
    fprintf(stderr, "libfuse-ll-hello open ino=%llu\n", (unsigned long long)ino);
    fflush(stderr);
    if (ino != 2) {
        fuse_reply_err(req, EISDIR);
    } else if ((fi->flags & 3) != O_RDONLY) {
        fuse_reply_err(req, EACCES);
    } else {
        fuse_reply_open(req, fi);
    }
}

static void hello_read(fuse_req_t req, fuse_ino_t ino, size_t size, off_t off, struct fuse_file_info *fi) {
    (void)fi;
    fprintf(stderr, "libfuse-ll-hello read ino=%llu off=%lld size=%zu\n", (unsigned long long)ino, (long long)off, size);
    fflush(stderr);
    if (ino != 2) {
        fuse_reply_err(req, ENOENT);
        return;
    }
    reply_limited(req, hello_body, strlen(hello_body), off, size);
}

static struct fuse_lowlevel_ops ops = {
    .init = hello_init,
    .destroy = hello_destroy,
    .lookup = hello_lookup,
    .getattr = hello_getattr,
    .statfs = hello_statfs,
    .readdir = hello_readdir,
    .open = hello_open,
    .read = hello_read,
};

int main(int argc, char **argv) {
    struct fuse_args args = FUSE_ARGS_INIT(argc, argv);
    struct fuse_chan *chan;
    struct fuse_session *session;
    char *mountpoint = NULL;
    int err = 1;

    if (fuse_parse_cmdline(&args, &mountpoint, NULL, NULL) == -1) {
        return 1;
    }
    chan = fuse_mount(mountpoint, &args);
    if (chan == NULL) {
        return 1;
    }
    session = fuse_lowlevel_new(&args, &ops, sizeof(ops), NULL);
    if (session != NULL) {
        if (fuse_set_signal_handlers(session) != -1) {
            fuse_session_add_chan(session, chan);
            err = fuse_session_loop(session);
            fuse_remove_signal_handlers(session);
            fuse_session_remove_chan(chan);
        }
        fuse_session_destroy(session);
    }
    fuse_unmount(mountpoint, chan);
    fuse_opt_free_args(&args);
    return err ? 1 : 0;
}
C
}

wait_for_probe_mount() {
  for _ in $(seq 1 30); do
    if [[ -n "$PROBE_PID" ]] && ! kill -0 "$PROBE_PID" >/dev/null 2>&1; then
      echo "libfuse low-level hello process exited before exposing hello.txt" >&2
      dump_diagnostics
      return 1
    fi
    if [[ -f "$MOUNT_DIR/hello.txt" ]]; then
      return 0
    fi
    sleep 1
  done
  echo "libfuse low-level hello did not expose hello.txt" >&2
  dump_diagnostics
  return 1
}

echo "macOS libfuse low-level hello backend: ${OPERON_MOUNT_MACOS_BACKEND:-nfs}" >&2
echo "macOS libfuse low-level hello extra options: ${OPERON_MOUNT_MACOS_OPTIONS:-<none>}" >&2
if ! pkg-config --modversion fuse >/dev/null 2>&1; then
  echo "pkg-config cannot resolve fuse; install FUSE-T before running libfuse low-level hello probe" >&2
  exit 1
fi

start_watchdog
write_probe_source
cc -Wall -Wextra -Werror -D_FILE_OFFSET_BITS=64 $(pkg-config --cflags fuse) "$SRC_PATH" $(pkg-config --libs fuse) -o "$BIN_PATH"
sudo mkdir -p "$MOUNT_DIR"
sudo chown "$(id -u):$(id -g)" "$MOUNT_DIR"

mount_options="backend=${OPERON_MOUNT_MACOS_BACKEND:-nfs}"
if [[ -n "${OPERON_MOUNT_MACOS_OPTIONS:-}" ]]; then
  mount_options="${mount_options},${OPERON_MOUNT_MACOS_OPTIONS}"
fi

"$BIN_PATH" -f -o "$mount_options" "$MOUNT_DIR" >"$PROBE_LOG" 2>&1 &
PROBE_PID="$!"
wait_for_probe_mount
grep -q "^hello from libfuse lowlevel$" "$MOUNT_DIR/hello.txt"
echo "=== libfuse low-level hello log ===" >&2
cat "$PROBE_LOG" >&2

echo "macOS FUSE-T libfuse low-level hello probe passed"
