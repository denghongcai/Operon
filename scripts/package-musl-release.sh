#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
if [[ $# != 3 ]]; then
  echo "usage: $0 <musl-target> <binary-directory> <output-directory>" >&2; exit 2
fi
case "$1" in
  x86_64-unknown-linux-musl) arch=x86_64 ;;
  aarch64-unknown-linux-musl) arch=arm64 ;;
  *) echo "unsupported musl target: $1" >&2; exit 2 ;;
esac
binary_dir="$(cd "$2" && pwd)"
mkdir -p "$3"
output_dir="$(cd "$3" && pwd)"
bash scripts/verify-musl-static-binaries.sh "$1" "$binary_dir/operon" "$binary_dir/operond"
version="$("$binary_dir/operon" --version)"
version="${version#operon }"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo 'invalid public version' >&2; exit 1; }
[[ "$("$binary_dir/operond" --version)" == "operond $version" ]] || {
  echo 'CLI and daemon versions differ' >&2; exit 1;
}
name="operon-v${version}-linux-musl-${arch}"
task_dir="$(mktemp -d)"
trap 'rm -rf "$task_dir"' EXIT
mkdir -p "$task_dir/$name"
cp "$binary_dir/operon" "$binary_dir/operond" README.md PROTOCOL.md LICENSE "$task_dir/$name/"
tar -C "$task_dir" -czf "$output_dir/$name.tar.gz" "$name"
bash scripts/smoke-release-archive.sh "$output_dir/$name.tar.gz"
sha256sum "$output_dir/$name.tar.gz"
