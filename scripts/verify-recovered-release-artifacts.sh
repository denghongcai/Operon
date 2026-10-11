#!/usr/bin/env bash
# Exceptional upload recovery: prove public bytes equal a fully accepted build.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/release-assets.sh"
TAG="${1:?release tag required}"
SOURCE_SHA="${2:?full source SHA required}"
REPO="${3:?repository required}"
RUN_ID="${4:?original build run required}"
release_validate_tag "$TAG"
[[ "$SOURCE_SHA" =~ ^[0-9a-f]{40}$ && "$RUN_ID" =~ ^[0-9]+$ ]]
[[ "$(gh api "repos/$REPO/commits/$TAG" --jq .sha)" == "$SOURCE_SHA" ]]
WORKDIR="$(mktemp -d /tmp/operon-release-recovery.XXXXXX)"
trap 'rm -rf "$WORKDIR"' EXIT
gh run view "$RUN_ID" --repo "$REPO" --json headSha,headBranch,name,status,conclusion,jobs > "$WORKDIR/run.json"
node - "$WORKDIR/run.json" "$SOURCE_SHA" "$TAG" <<'JS'
const fs = require('node:fs');
const [file, sha, tag] = process.argv.slice(2);
const run = JSON.parse(fs.readFileSync(file, 'utf8'));
const required = [
  'Release Gates',
  ...['linux-arm64','linux-armv7','linux-x86_64'].map(x => `Build Rust Linux (${x})`),
  ...['macos-aarch64','macos-x86_64','windows-x86_64'].map(x => `Build Rust Native (${x})`),
  'Build TypeScript SDK',
  ...['aarch64','x86_64'].map(x => `Build and Accept Native musl Archives / Native musl Build (${x})`),
  'build-sdk-native / linux-package (ubuntu-24.04, x86_64-unknown-linux-gnu)',
  'build-sdk-native / linux-package (ubuntu-24.04-arm, aarch64-unknown-linux-gnu)',
  'build-sdk-native / package (macos-15, aarch64-apple-darwin)',
  'build-sdk-native / package (macos-15-intel, x86_64-apple-darwin)',
  'build-sdk-native / package (windows-2025, x86_64-pc-windows-msvc, .exe)',
];
if (run.headSha !== sha || run.headBranch !== tag || run.name !== 'Draft Release' ||
    run.status !== 'completed' || run.conclusion !== 'failure' || run.jobs.length !== required.length + 1)
  throw new Error('Not an exact-source upload-only recovery candidate');
for (const name of required) {
  const jobs = run.jobs.filter(x => x.name === name);
  if (jobs.length !== 1 || jobs[0].conclusion !== 'success')
    throw new Error(`Required build/acceptance did not pass: ${name}`);
}
const draft = run.jobs.filter(x => x.name === 'Draft GitHub Release');
if (draft.length !== 1 || draft[0].conclusion !== 'failure') throw new Error('Unexpected failure');
JS
# Require the known directory-upload error, not an unrelated packaging failure.
gh run view "$RUN_ID" --repo "$REPO" --log-failed > "$WORKDIR/failure.log"
rg -F 'read dist/dist: is a directory' "$WORKDIR/failure.log" >/dev/null
mkdir "$WORKDIR/build" "$WORKDIR/public"
gh run download "$RUN_ID" --repo "$REPO" --dir "$WORKDIR/build"
gh release download "$TAG" --repo "$REPO" --dir "$WORKDIR/public"
while IFS= read -r asset; do
  test -f "$WORKDIR/public/$asset"
  [[ "$asset" != SHA256SUMS ]] || continue
  mapfile -d '' matches < <(find "$WORKDIR/build" -type f -name "$asset" -print0)
  [[ ${#matches[@]} == 1 ]] || { echo "Missing/duplicate build asset: $asset" >&2; exit 1; }
  cmp -- "${matches[0]}" "$WORKDIR/public/$asset"
done < <(release_expected_assets "$TAG")
diff -u <(release_expected_assets "$TAG" | sort) <(find "$WORKDIR/public" -maxdepth 1 -type f -printf '%f\n' | sort)
(cd "$WORKDIR/public" && sha256sum -c SHA256SUMS)
echo "Recovered release bytes match accepted exact-source build $RUN_ID ($SOURCE_SHA)"
