#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "v0.14 FUSE-T install requires macOS" >&2
  exit 1
fi

if ! command -v brew >/dev/null 2>&1; then
  echo "Homebrew is required to install FUSE-T in this workflow" >&2
  exit 1
fi

brew install macos-fuse-t/homebrew-cask/fuse-t

if pkg-config --modversion fuse >/dev/null 2>&1; then
  pkg-config --modversion fuse
  pkg-config --libs fuse
  exit 0
fi

find_first() {
  local pattern="$1"
  shift
  for root in "$@"; do
    if [[ -d "$root" ]]; then
      find "$root" -name "$pattern" -print -quit
    fi
  done
}

lib_path="$(find_first libfuse-t.dylib /usr/local/lib /opt/homebrew/lib)"
header_path="$(find_first fuse.h /usr/local/include /opt/homebrew/include)"

if [[ -z "$lib_path" || -z "$header_path" ]]; then
  echo "FUSE-T installed, but pkg-config fuse metadata is unavailable and compatibility metadata could not be generated" >&2
  echo "libfuse-t: ${lib_path:-missing}" >&2
  echo "fuse.h: ${header_path:-missing}" >&2
  exit 1
fi

lib_dir="$(dirname "$lib_path")"
include_dir="$(dirname "$header_path")"
pkgconfig_dir="$lib_dir/pkgconfig"
pc_file="$pkgconfig_dir/fuse.pc"

sudo mkdir -p "$pkgconfig_dir"
sudo tee "$pc_file" >/dev/null <<PC
prefix=${lib_dir%/lib}
exec_prefix=\${prefix}
libdir=${lib_dir}
includedir=${include_dir}

Name: fuse
Description: FUSE-T compatibility metadata for libfuse
Version: 2.9.9
Libs: -L\${libdir} -lfuse-t
Cflags: -I\${includedir}
PC

export PKG_CONFIG_PATH="$pkgconfig_dir:${PKG_CONFIG_PATH:-}"
if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "PKG_CONFIG_PATH=$PKG_CONFIG_PATH" >>"$GITHUB_ENV"
fi
pkg-config --modversion fuse
pkg-config --libs fuse
