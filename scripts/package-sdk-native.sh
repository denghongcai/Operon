#!/usr/bin/env bash
set -euo pipefail
if [[ $# != 3 ]]; then
  echo "usage: $0 <rust-target> <binding-directory> <archive-directory>" >&2
  exit 2
fi
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sdk_target=$1
binding_dir=$2
archive_dir=$3
case "$sdk_target" in
  x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|armv7-unknown-linux-gnueabihf|x86_64-unknown-linux-musl|aarch64-unknown-linux-musl|x86_64-apple-darwin|aarch64-apple-darwin|x86_64-pc-windows-msvc) ;;
  *) echo "unsupported native SDK target: $sdk_target" >&2; exit 2 ;;
esac
test -f "$binding_dir/operon-sdk-native.node"
sdk_version=$(node -p "JSON.parse(require('fs').readFileSync(process.argv[1], 'utf8')).version" "$repo_root/packages/sdk-js/package.json")
[[ "$sdk_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
stage_dir=$(mktemp -d /tmp/operon-sdk-package.XXXXXXXX)
trap 'rm -rf -- "$stage_dir"' EXIT
cp "$binding_dir/operon-sdk-native.node" "$stage_dir/operon-sdk-native.node"
node --input-type=module - "$stage_dir" "$sdk_target" "$sdk_version" <<'JS'
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
const [directory, target, version] = process.argv.slice(2);
const binding = { exports: {} };
process.dlopen(binding, join(directory, 'operon-sdk-native.node'));
if (binding.exports.bindingVersion?.() !== version) {
  throw new Error('Native binding does not match the SDK version; run packaging on its native platform');
}
const sha256 = createHash('sha256').update(readFileSync(join(directory, 'operon-sdk-native.node'))).digest('hex');
writeFileSync(join(directory, 'binding.json'), JSON.stringify({ version, target, napi: 8, file: 'operon-sdk-native.node', sha256 }, null, 2) + '\n');
JS
mkdir -p "$archive_dir"
archive="$archive_dir/operon-sdk-native-v$sdk_version-$sdk_target.tar.gz"
tar -czf "$archive" -C "$stage_dir" operon-sdk-native.node binding.json
test "$(tar -tzf "$archive" | wc -l | tr -d ' ')" = 2
echo "Packaged optional binding: $archive"
