#!/usr/bin/env bash
# Shared immutable public-version asset contract and runtime libc selection.

release_validate_tag() {
  [[ "$1" =~ ^v[0-9]{1,8}\.[0-9]{1,8}(\.[0-9]{1,8})?(-[A-Za-z0-9.-]+)?$ ]] || {
    echo "invalid release tag: $1" >&2; return 2;
  }
}

release_has_musl_assets() {
  local tag="$1" major minor patch
  release_validate_tag "$tag" || return 2
  [[ "$tag" =~ ^v([0-9]+)\.([0-9]+)(\.([0-9]+))? ]]
  major=$((10#${BASH_REMATCH[1]}))
  minor=$((10#${BASH_REMATCH[2]}))
  patch=$((10#${BASH_REMATCH[4]:-0}))
  (( major > 0 || minor > 16 || (minor == 16 && patch >= 12) ))
}

release_expected_version() {
  release_validate_tag "$1" || return 2
  [[ "$1" =~ ^v([0-9]+)\.([0-9]+)(\.([0-9]+))?(-[A-Za-z0-9.-]+)?$ ]]
  printf '%s.%s.%s%s\n' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "${BASH_REMATCH[4]:-0}" "${BASH_REMATCH[5]:-}"
}

release_expected_assets() {
  local tag="$1"
  release_validate_tag "$tag" || return 2
  cat <<ASSETS
operon-${tag}-linux-x86_64.tar.gz
operon-${tag}-linux-arm64.tar.gz
operon-${tag}-linux-armv7.tar.gz
operon-${tag}-macos-x86_64.tar.gz
operon-${tag}-macos-aarch64.tar.gz
operon-${tag}-windows-x86_64.zip
operon-sdk-js-${tag}.tar.gz
SHA256SUMS
ASSETS
  if release_has_musl_assets "$tag"; then
    printf 'operon-%s-linux-musl-x86_64.tar.gz\noperon-%s-linux-musl-arm64.tar.gz\n' "$tag" "$tag"
  fi
}

release_detect_linux_libc() {
  local requested="${OPERON_RELEASE_LIBC:-auto}" glibc=false musl=false output
  case "$requested" in
    gnu|musl) printf '%s\n' "$requested"; return ;;
    auto) ;;
    *) echo 'OPERON_RELEASE_LIBC must be auto, gnu or musl' >&2; return 1 ;;
  esac
  # Inspect runtime tools/distro, not the presence of a musl development loader:
  # installing musl-gcc on a glibc host must not change the selected archive.
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
  release_resolve_linux_libc "$glibc" "$musl"
}

release_resolve_linux_libc() {
  case "$1-$2" in
    true-false) echo gnu ;;
    false-true) echo musl ;;
    *) echo 'Linux libc is unknown or ambiguous; set OPERON_RELEASE_LIBC=gnu or musl explicitly' >&2; return 1 ;;
  esac
}

release_current_asset_name() {
  local tag="$1" system machine libc architecture
  release_validate_tag "$tag" || return 2
  system="$(uname -s)"
  machine="$(uname -m)"
  case "${system}-${machine}" in
    Linux-*)
      case "$machine" in
        x86_64) architecture=x86_64 ;;
        aarch64|arm64) architecture=arm64 ;;
        armv7l|armv7*) architecture=armv7 ;;
        *) echo "unsupported Linux architecture: $machine" >&2; return 1 ;;
      esac
      libc="$(release_detect_linux_libc)" || return 1
      if [[ "$libc" == musl ]]; then
        release_has_musl_assets "$tag" || { echo "$tag predates public musl assets; select v0.16.12 or newer" >&2; return 1; }
        [[ "$architecture" != armv7 ]] || { echo 'ARMv7 musl archives are not supported; no glibc fallback is performed' >&2; return 1; }
        printf 'operon-%s-linux-musl-%s.tar.gz\n' "$tag" "$architecture"
      else
        printf 'operon-%s-linux-%s.tar.gz\n' "$tag" "$architecture"
      fi
      ;;
    Darwin-x86_64) printf 'operon-%s-macos-x86_64.tar.gz\n' "$tag" ;;
    Darwin-arm64|Darwin-aarch64) printf 'operon-%s-macos-aarch64.tar.gz\n' "$tag" ;;
    MINGW64_NT-*|MSYS_NT-*|CYGWIN_NT-*|Windows_NT-*) printf 'operon-%s-windows-x86_64.zip\n' "$tag" ;;
    *) echo "unsupported release install platform: ${system}-${machine}" >&2; return 1 ;;
  esac
}
