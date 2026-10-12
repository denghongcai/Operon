#!/usr/bin/env bash
# Install the operon CLI and operond daemon from a public GitHub Release.
#
#   curl -fsSL https://raw.githubusercontent.com/denghongcai/Operon/main/scripts/install.sh | bash
#
# Environment:
#   OPERON_VERSION        release tag to install (default: latest release)
#   OPERON_INSTALL_DIR    target directory (default: /usr/local/bin)
#   OPERON_RELEASE_LIBC   Linux libc: auto (default), gnu or musl
#
# Pass --dry-run to print the resolved release asset without downloading it.
#
# Archive selection mirrors scripts/lib/release-assets.sh; keep them in sync.
set -euo pipefail

REPO="denghongcai/Operon"

fail() {
  echo "operon install: $*" >&2
  exit 1
}

has_musl_assets() {
  [[ "$1" =~ ^v([0-9]+)\.([0-9]+)(\.([0-9]+))? ]] || return 1
  local major=$((10#${BASH_REMATCH[1]})) minor=$((10#${BASH_REMATCH[2]})) patch=$((10#${BASH_REMATCH[4]:-0}))
  (( major > 0 || minor > 16 || (minor == 16 && patch >= 12) ))
}

detect_linux_libc() {
  local requested="${OPERON_RELEASE_LIBC:-auto}" glibc=false musl=false output
  case "$requested" in
    gnu|musl) echo "$requested"; return ;;
    auto) ;;
    *) fail 'OPERON_RELEASE_LIBC must be auto, gnu or musl' ;;
  esac
  if command -v getconf >/dev/null 2>&1; then
    output="$(LC_ALL=C getconf GNU_LIBC_VERSION 2>/dev/null || true)"
    [[ "$output" == glibc\ * ]] && glibc=true
  fi
  if command -v ldd >/dev/null 2>&1; then
    output="$(LC_ALL=C ldd --version 2>&1 || true)"
    [[ "$output" == *musl* ]] && musl=true
    [[ "$output" == *GLIBC* || "$output" == *'GNU libc'* || "$output" == *'GNU C Library'* ]] && glibc=true
  fi
  test ! -f /etc/alpine-release || musl=true
  case "$glibc-$musl" in
    true-false) echo gnu ;;
    false-true) echo musl ;;
    *) fail 'Linux libc is unknown or ambiguous; set OPERON_RELEASE_LIBC=gnu or musl explicitly' ;;
  esac
}

asset_name() {
  local tag="$1" system machine arch libc
  system="$(uname -s)"
  machine="$(uname -m)"
  case "$system-$machine" in
    Linux-*)
      case "$machine" in
        x86_64) arch=x86_64 ;;
        aarch64|arm64) arch=arm64 ;;
        armv7l|armv7*) arch=armv7 ;;
        *) fail "unsupported Linux architecture: $machine" ;;
      esac
      libc="$(detect_linux_libc)"
      if [[ "$libc" == musl ]]; then
        has_musl_assets "$tag" || fail "$tag predates public musl archives; use v0.16.12 or newer"
        [[ "$arch" != armv7 ]] || fail 'ARMv7 musl archives are not provided'
        echo "operon-$tag-linux-musl-$arch"
      else
        echo "operon-$tag-linux-$arch"
      fi
      ;;
    Darwin-x86_64) echo "operon-$tag-macos-x86_64" ;;
    Darwin-arm64|Darwin-aarch64) echo "operon-$tag-macos-aarch64" ;;
    *) fail "unsupported platform: $system-$machine (on Windows use scripts/install.ps1)" ;;
  esac
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    fail 'sha256sum or shasum is required to verify the download'
  fi
}

main() {
  local dry_run=false
  case "${1:-}" in
    --dry-run) dry_run=true ;;
    "") ;;
    *) fail "unknown argument: $1" ;;
  esac

  command -v curl >/dev/null 2>&1 || fail 'curl is required'
  command -v tar >/dev/null 2>&1 || fail 'tar is required'

  local version="${OPERON_VERSION:-}"
  if [[ -z "$version" ]]; then
    # Follow the releases/latest redirect instead of the rate-limited REST API.
    version="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")"
    version="${version##*/}"
    [[ -n "$version" ]] || fail 'failed to resolve the latest release; set OPERON_VERSION'
  fi
  [[ "$version" =~ ^v[0-9]+\.[0-9]+(\.[0-9]+)?(-[A-Za-z0-9.-]+)?$ ]] || fail "invalid release tag: $version"

  local name install_dir
  name="$(asset_name "$version")"
  install_dir="${OPERON_INSTALL_DIR:-/usr/local/bin}"

  if [[ "$dry_run" == true ]]; then
    echo "version=$version"
    echo "asset=$name.tar.gz"
    echo "install_dir=$install_dir"
    return
  fi

  local base="https://github.com/$REPO/releases/download/$version" work
  work="$(mktemp -d)"
  WORK_DIR="$work"
  trap 'rm -rf "${WORK_DIR:-}"' EXIT

  echo "Downloading $name.tar.gz"
  curl -fsSL "$base/$name.tar.gz" -o "$work/$name.tar.gz"
  curl -fsSL "$base/SHA256SUMS" -o "$work/SHA256SUMS"
  local expected actual
  expected="$(awk -v f="$name.tar.gz" '$2 == f || $2 == "*" f { print $1 }' "$work/SHA256SUMS")"
  [[ -n "$expected" ]] || fail "$name.tar.gz is missing from SHA256SUMS"
  actual="$(sha256 "$work/$name.tar.gz")"
  [[ "$expected" == "$actual" ]] || fail "checksum mismatch for $name.tar.gz"

  tar -xzf "$work/$name.tar.gz" -C "$work"

  local sudo=""
  if ! mkdir -p "$install_dir" 2>/dev/null || [[ ! -w "$install_dir" ]]; then
    command -v sudo >/dev/null 2>&1 || fail "$install_dir is not writable; set OPERON_INSTALL_DIR"
    sudo="sudo"
    $sudo mkdir -p "$install_dir"
  fi
  $sudo install "$work/$name/operon" "$install_dir/operon"
  $sudo install "$work/$name/operond" "$install_dir/operond"
  # macOS archives bundle FUSE-T's client library next to the binaries.
  if [[ -f "$work/$name/libfuse-t.dylib" ]]; then
    $sudo install -m 0644 "$work/$name/libfuse-t.dylib" "$install_dir/libfuse-t.dylib"
  fi

  echo "Installed operon and operond $version to $install_dir"
  case ":$PATH:" in
    *":$install_dir:"*) ;;
    *) echo "Add $install_dir to PATH to use them." ;;
  esac
  echo "Live mount also needs FUSE (Linux) or FUSE-T (macOS); check with: operon doctor --mount-runtime"
}

main "$@"
