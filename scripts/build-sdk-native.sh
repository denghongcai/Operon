#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
if [[ $# != 2 ]]; then
  echo "usage: $0 <native-rust-target> <output-directory>" >&2
  exit 2
fi
build_target=$1
output_dir=$2
toolchain=${OPERON_RUST_TOOLCHAIN:-1.91.0}
build_profile=${OPERON_SDK_BUILD_PROFILE:-release}
case "$build_profile" in
  release) artifact_profile=release ;;
  dev) artifact_profile=debug ;;
  *) echo 'SDK build profile must be dev or release' >&2; exit 2 ;;
esac
case "$build_target" in
  x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|armv7-unknown-linux-gnueabihf|x86_64-unknown-linux-musl|aarch64-unknown-linux-musl)
    library=liboperon_sdk_native.so ;;
  x86_64-apple-darwin|aarch64-apple-darwin)
    library=liboperon_sdk_native.dylib ;;
  x86_64-pc-windows-msvc)
    library=operon_sdk_native.dll ;;
  *) echo "unsupported native SDK target: $build_target" >&2; exit 2 ;;
esac
# Cross-builds need native C toolchains/SDKs; mandatory acceptance uses native
# runners. Dynamic musl belongs only to the Node addon, not daemon/CLI builds.
if [[ "$build_target" == *-musl ]]; then
  RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=-crt-static" \
    cargo "+$toolchain" build -p operon-sdk-native --features node --locked \
      --target "$build_target" --profile "$build_profile"
else
  cargo "+$toolchain" build -p operon-sdk-native --features node --locked \
    --target "$build_target" --profile "$build_profile"
fi
artifact="${CARGO_TARGET_DIR:-target}/$build_target/$artifact_profile/$library"
test -f "$artifact"
mkdir -p "$output_dir"
cp "$artifact" "$output_dir/operon-sdk-native.node"
echo "Built optional SDK binding: $output_dir/operon-sdk-native.node ($build_target)"
