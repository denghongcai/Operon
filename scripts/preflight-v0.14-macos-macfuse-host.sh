#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "v0.14 macOS macFUSE preflight requires macOS" >&2
  exit 1
fi

BACKEND="${OPERON_MOUNT_MACOS_BACKEND:-fskit}"
MACFUSE_ROOT="/Library/Filesystems/macfuse.fs"
MACFUSE_BIN="$MACFUSE_ROOT/Contents/Resources/macfuse.app/Contents/MacOS/macfuse"

if [[ "$BACKEND" != "fskit" && "$BACKEND" != "kernel" ]]; then
  echo "unsupported OPERON_MOUNT_MACOS_BACKEND: $BACKEND" >&2
  echo "expected fskit or kernel" >&2
  exit 1
fi

echo "macOS macFUSE preflight backend: $BACKEND"
sw_vers
uname -a

if [[ ! -d "$MACFUSE_ROOT" ]]; then
  echo "macFUSE runtime not found at $MACFUSE_ROOT" >&2
  echo "install macFUSE and approve/load it before running v0.14 live mount smoke" >&2
  exit 1
fi

if ! pkg-config --modversion fuse; then
  echo "pkg-config cannot resolve fuse for macFUSE" >&2
  echo "install macFUSE development metadata before running v0.14 live mount smoke" >&2
  exit 1
fi

if [[ -f "$MACFUSE_ROOT/Contents/Info.plist" ]]; then
  /usr/bin/defaults read "$MACFUSE_ROOT/Contents/Info.plist" CFBundleShortVersionString || true
fi

loaded="no"
if kmutil showloaded --list-only 2>/dev/null | grep -qi macfuse; then
  loaded="yes"
elif kextstat 2>/dev/null | grep -qi macfuse; then
  loaded="yes"
fi
echo "macFUSE kernel extension loaded: $loaded"

if [[ "$BACKEND" == "kernel" && "$loaded" != "yes" ]]; then
  echo "macFUSE kernel backend requires the macFUSE kernel extension to be approved and loaded" >&2
  echo "load/approve macFUSE on this host, then rerun the preflight and live smoke" >&2
  exit 1
fi

if [[ "$BACKEND" == "fskit" ]]; then
  version="$(sw_vers -productVersion)"
  major="${version%%.*}"
  rest="${version#*.}"
  minor="${rest%%.*}"
  if (( major < 15 || (major == 15 && minor < 4) )); then
    echo "macFUSE FSKit backend requires macOS 15.4 or newer; current version is $version" >&2
    echo "use OPERON_MOUNT_MACOS_BACKEND=kernel on older macOS hosts with an approved kernel extension" >&2
    exit 1
  fi
  if [[ ! -x "$MACFUSE_BIN" ]]; then
    echo "macFUSE FSKit management binary not found at $MACFUSE_BIN" >&2
    exit 1
  fi
fi

echo "v0.14 macOS macFUSE host preflight passed"
