#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/release-install.sh"
[[ $# == 2 ]] || { echo "usage: $0 <public-tag> <owner/repo>" >&2; exit 2; }
tag="$1"
repo="$2"
release_validate_tag "$tag"
release_has_musl_assets "$tag" || { echo 'this release predates musl assets' >&2; exit 1; }
[[ "$(uname -s)" == Linux ]] || { echo 'native Linux runner is required' >&2; exit 1; }
case "$(uname -m)" in
  x86_64) target=x86_64-unknown-linux-musl ;;
  aarch64) target=aarch64-unknown-linux-musl ;;
  *) echo 'native x86_64 or arm64 runner required; emulation does not satisfy the gate' >&2; exit 1 ;;
esac
asset="$(OPERON_RELEASE_LIBC=musl release_current_asset_name "$tag")"
[[ "$(gh release view "$tag" --repo "$repo" --json isDraft --jq '.isDraft')" == false ]] || {
  echo 'Alpine public verification requires a published release, not a draft' >&2; exit 1;
}
workdir="$(mktemp -d /tmp/operon-alpine-release.XXXXXX)"
trap 'rm -rf "$workdir"' EXIT
mkdir "$workdir/assets" "$workdir/extracted"
release_url="https://github.com/$repo/releases/download/$tag"
release_install_download "$release_url/SHA256SUMS" "$workdir/assets/SHA256SUMS"
release_install_download "$release_url/$asset" "$workdir/assets/$asset"
awk -v asset="$asset" '$2 == asset || $2 == "*" asset {print}' "$workdir/assets/SHA256SUMS" >"$workdir/assets/SHA256SUMS.current"
[[ "$(wc -l <"$workdir/assets/SHA256SUMS.current")" == 1 ]] || { echo 'checksum manifest must contain exactly one native musl archive entry' >&2; exit 1; }
(cd "$workdir/assets" && sha256sum -c SHA256SUMS.current)
bash "$ROOT/scripts/smoke-release-archive.sh" "$workdir/assets/$asset"
tar -xzf "$workdir/assets/$asset" -C "$workdir/extracted"
binary_directory="$workdir/extracted/${asset%.tar.gz}"
test -f "$binary_directory/LICENSE"
bash "$ROOT/scripts/verify-musl-static-binaries.sh" "$target" "$binary_directory/operon" "$binary_directory/operond"
sha256sum "$binary_directory/operon" "$binary_directory/operond"
if ! test -c /dev/fuse; then
  echo 'required native /dev/fuse is missing; this is not a successful skip' >&2; exit 1
fi
for image in \
  alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8 \
  alpine@sha256:85fe1e81d6758c208f3e1eed4338a1997e19d4be002d4dd32d3100c9a8c010a0; do
  docker run --rm \
    --mount "type=bind,src=$ROOT,dst=/workspace,readonly" \
    --mount "type=bind,src=$binary_directory,dst=/operon,readonly" \
    -e OPERON_INTEGRATION_BIN_DIR=/operon \
    -e OPERON_RELEASE_CONNECT_TIMEOUT_SECS="${OPERON_RELEASE_CONNECT_TIMEOUT_SECS:-30}" \
    -e OPERON_RELEASE_DOWNLOAD_TIMEOUT_SECS="${OPERON_RELEASE_DOWNLOAD_TIMEOUT_SECS:-0}" \
    "$image" /bin/sh -ec '
      apk add --no-cache bash python3 coreutils curl ca-certificates procps tar unzip >/dev/null
      cd /workspace
      # Real public install auto-selects musl inside Alpine; no override hides detection.
      bash scripts/verify-release-install-usability.sh "$0" "$1"
      bash scripts/verify-v0.8.1-integration-coverage.sh
    ' "$tag" "$repo"
  for identity in root nonroot; do
    docker run --rm --device /dev/fuse --cap-add SYS_ADMIN --security-opt apparmor=unconfined \
      --mount "type=bind,src=$ROOT,dst=/workspace,readonly" \
      --mount "type=bind,src=$binary_directory,dst=/operon,readonly" \
      -e OPERON_ALPINE_IDENTITY="$identity" \
      -e OPERON_ALPINE_ACCEPT_TIMEOUT_SECS="${OPERON_ALPINE_ACCEPT_TIMEOUT_SECS:-120}" \
      -e OPERON_ALPINE_STOP_TIMEOUT_SECS="${OPERON_ALPINE_STOP_TIMEOUT_SECS:-60}" \
      "$image" /bin/sh -ec '
        apk add --no-cache python3 fuse3 >/dev/null
        if [ "$OPERON_ALPINE_IDENTITY" = nonroot ]; then
          adduser -D -u 1000 operon-test
          su operon-test -s /bin/sh -c "python3 /workspace/scripts/alpine-runtime-acceptance.py --bin-dir /operon --live-mount"
        else
          python3 /workspace/scripts/alpine-runtime-acceptance.py --bin-dir /operon --live-mount
        fi
      '
  done
  bash "$ROOT/scripts/run-alpine-openrc-acceptance.sh" "$binary_directory" "$image"
done
gnu_asset="$(OPERON_RELEASE_LIBC=gnu release_current_asset_name "$tag")"
release_install_download "$release_url/$gnu_asset" "$workdir/assets/$gnu_asset"
awk -v asset="$gnu_asset" '$2 == asset || $2 == "*" asset {print}' "$workdir/assets/SHA256SUMS" >"$workdir/assets/SHA256SUMS.gnu"
[[ "$(wc -l <"$workdir/assets/SHA256SUMS.gnu")" == 1 ]] || { echo 'checksum manifest must contain exactly one native GNU archive entry' >&2; exit 1; }
(cd "$workdir/assets" && sha256sum -c SHA256SUMS.gnu)
bash "$ROOT/scripts/smoke-release-archive.sh" "$workdir/assets/$gnu_asset"
tar -xzf "$workdir/assets/$gnu_asset" -C "$workdir/extracted"
bash "$ROOT/scripts/run-libc-comparison.sh" "$workdir/extracted/${gnu_asset%.tar.gz}" "$binary_directory"
echo "public native Alpine install/runtime/FUSE/OpenRC passed for $repo@$tag ($target)"
