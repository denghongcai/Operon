#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
if [[ $# != 1 ]]; then
  echo "usage: $0 <x86_64-unknown-linux-musl|aarch64-unknown-linux-musl>" >&2
  exit 2
fi
case "$1" in
  x86_64-unknown-linux-musl) arch=x86_64 ;;
  aarch64-unknown-linux-musl) arch=aarch64 ;;
  *) echo "unsupported musl target: $1" >&2; exit 2 ;;
esac
[[ "$(uname -m)" == "$arch" ]] || {
  echo "this reproducible builder requires a native $arch runner; cross-builds are not runtime evidence" >&2
  exit 1
}
command -v cc >/dev/null || { echo 'C compiler is required' >&2; exit 1; }
command -v protoc >/dev/null || { echo 'protoc is required' >&2; exit 1; }
command -v readelf >/dev/null || { echo 'readelf is required' >&2; exit 1; }
toolchain="${OPERON_MUSL_RUST_TOOLCHAIN:-1.88.0}"
output="${CARGO_TARGET_DIR:-$ROOT/target}"
echo "native architecture: $arch"
rustc "+$toolchain" --version
cc --version
protoc --version
# Rust's bundled musl CRT provides static linking on GNU builders as well.
# Do not substitute musl-gcc: older wrapper specs can insert an interpreter
# into static-PIE output. The ELF verifier below is the authoritative gate.
export CARGO_INCREMENTAL=0
case "$arch" in
  x86_64) export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=cc ;;
  aarch64) export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=cc ;;
esac
cargo "+$toolchain" build --release --locked --target "$1" -p operon-cli -p operond
bash scripts/verify-musl-static-binaries.sh "$1" \
  "$output/$1/release/operon" "$output/$1/release/operond"
sha256sum "$output/$1/release/operon" "$output/$1/release/operond"
