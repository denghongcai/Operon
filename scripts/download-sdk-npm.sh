#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/release-assets.sh"
tag="${1:?release tag required}"
destination="${2:?new SDK directory required}"
version="$(release_expected_version "$tag")"
mkdir "$destination"
archive_dir="$(mktemp -d /tmp/operon-npm-download.XXXXXX)"
trap 'rm -rf "$archive_dir"' EXIT
npm pack "@cellworks/operon-sdk-js@$version" --pack-destination "$archive_dir" --ignore-scripts --registry=https://registry.npmjs.org/ --prefer-online >/dev/null
tar -xzf "$archive_dir/cellworks-operon-sdk-js-${version}.tgz" --strip-components=1 -C "$destination"
node - "$destination" "$version" <<'JS'
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const [directory, version] = process.argv.slice(2);
const manifest = JSON.parse(fs.readFileSync(path.join(directory, 'package.json'), 'utf8'));
assert.equal(manifest.name, '@cellworks/operon-sdk-js');
assert.equal(manifest.version, version);
assert(fs.statSync(path.join(directory, 'dist/index.js')).isFile());
JS
while IFS= read -r target; do
  node "$ROOT/scripts/verify-sdk-native-manifest.mjs" "$destination/native/$target" "$version" "$target"
done < <(release_sdk_native_targets)
