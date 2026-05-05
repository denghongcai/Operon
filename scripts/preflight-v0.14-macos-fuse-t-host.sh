#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "v0.14 macOS FUSE-T preflight requires macOS" >&2
  exit 1
fi

BACKEND="${OPERON_MOUNT_MACOS_BACKEND:-nfs}"

if [[ "$BACKEND" != "nfs" && "$BACKEND" != "smb" && "$BACKEND" != "fskit" ]]; then
  echo "unsupported OPERON_MOUNT_MACOS_BACKEND: $BACKEND" >&2
  echo "expected nfs, smb, or fskit" >&2
  exit 1
fi

echo "macOS FUSE-T preflight backend: $BACKEND"
sw_vers
uname -a

if ! pkg-config --modversion fuse; then
  echo "pkg-config cannot resolve fuse for FUSE-T" >&2
  echo "install FUSE-T before running v0.14 live mount smoke" >&2
  exit 1
fi

pkg-config --libs fuse || true
pkg-config --cflags fuse || true

if pkg-config --libs fuse 2>/dev/null | grep -qi 'fuse-t'; then
  echo "pkg-config fuse links FUSE-T"
elif [[ -f /usr/local/lib/libfuse-t.dylib || -f /opt/homebrew/lib/libfuse-t.dylib ]]; then
  echo "FUSE-T library found"
else
  echo "FUSE-T library not found in /usr/local/lib or /opt/homebrew/lib" >&2
  echo "install FUSE-T before running v0.14 live mount smoke" >&2
  exit 1
fi

if [[ "$BACKEND" == "nfs" ]]; then
  nfsd status || true
fi

echo "v0.14 macOS FUSE-T host preflight passed"
