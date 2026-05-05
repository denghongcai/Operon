#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/verify-release-artifacts.sh <tag> [owner/repo]
  scripts/verify-release-artifacts.sh --dry-run <tag>

Downloads GitHub Release assets, validates SHA256SUMS, verifies the expected
artifact set, and smoke-tests the archive for the current platform.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN=true
  shift
fi

TAG="${1:-}"
REPO="${2:-${GITHUB_REPOSITORY:-}}"

if [[ -z "$TAG" ]]; then
  usage
  exit 1
fi

if [[ -z "$REPO" ]]; then
  if remote_url="$(git remote get-url origin 2>/dev/null)"; then
    REPO="$(printf '%s\n' "$remote_url" \
      | sed -E 's#^git@github.com:##; s#^https://github.com/##; s#\.git$##')"
  fi
fi

if [[ -z "$REPO" ]]; then
  echo "failed to determine GitHub repository; pass owner/repo explicitly" >&2
  exit 1
fi

expected_assets() {
  local tag="$1"
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
}

current_asset_name() {
  local tag="$1"
  local system machine
  system="$(uname -s)"
  machine="$(uname -m)"
  case "${system}-${machine}" in
    Linux-x86_64) printf 'operon-%s-linux-x86_64.tar.gz\n' "$tag" ;;
    Linux-aarch64|Linux-arm64) printf 'operon-%s-linux-arm64.tar.gz\n' "$tag" ;;
    Linux-armv7l|Linux-armv7*) printf 'operon-%s-linux-armv7.tar.gz\n' "$tag" ;;
    Darwin-x86_64) printf 'operon-%s-macos-x86_64.tar.gz\n' "$tag" ;;
    Darwin-arm64) printf 'operon-%s-macos-aarch64.tar.gz\n' "$tag" ;;
    MINGW64_NT-*|MSYS_NT-*|CYGWIN_NT-*|Windows_NT-*) printf 'operon-%s-windows-x86_64.zip\n' "$tag" ;;
    *) echo "unsupported release verification platform: ${system}-${machine}" >&2; return 1 ;;
  esac
}

if [[ "$DRY_RUN" == true ]]; then
  echo "repo=$REPO"
  echo "tag=$TAG"
  expected_assets "$TAG"
  current_asset_name "$TAG" >/dev/null
  exit 0
fi

command -v gh >/dev/null || {
  echo "gh is required to download release assets" >&2
  exit 1
}
command -v sha256sum >/dev/null || {
  echo "sha256sum is required to verify release assets" >&2
  exit 1
}

WORKDIR="${OPERON_RELEASE_VERIFY_DIR:-$(mktemp -d)}"
trap 'rm -rf "$WORKDIR"' EXIT
mkdir -p "$WORKDIR/assets" "$WORKDIR/extracted"

gh release download "$TAG" --repo "$REPO" --dir "$WORKDIR/assets" --pattern '*'

while IFS= read -r asset; do
  test -f "$WORKDIR/assets/$asset" || {
    echo "missing expected release asset: $asset" >&2
    exit 1
  }
done < <(expected_assets "$TAG")

unexpected="$(
  comm -23 \
    <((cd "$WORKDIR/assets" && for path in *; do test -f "$path" && printf '%s\n' "$path"; done) | sort) \
    <(expected_assets "$TAG" | sort)
)"
if [[ -n "$unexpected" ]]; then
  echo "unexpected release assets:" >&2
  printf '%s\n' "$unexpected" >&2
  exit 1
fi

(
  cd "$WORKDIR/assets"
  sha256sum -c SHA256SUMS
)

asset="$(current_asset_name "$TAG")"
case "$asset" in
  *.zip)
    command -v unzip >/dev/null || {
      echo "unzip is required to verify Windows archives" >&2
      exit 1
    }
    unzip -q "$WORKDIR/assets/$asset" -d "$WORKDIR/extracted"
    suffix=".exe"
    ;;
  *.tar.gz)
    tar -xzf "$WORKDIR/assets/$asset" -C "$WORKDIR/extracted"
    suffix=""
    ;;
  *)
    echo "unsupported archive format: $asset" >&2
    exit 1
    ;;
esac

archive_dir="$WORKDIR/extracted/${asset%.tar.gz}"
archive_dir="${archive_dir%.zip}"
operon_bin="$archive_dir/operon${suffix}"
operond_bin="$archive_dir/operond${suffix}"

test -f "$operon_bin" || { echo "missing binary: $operon_bin" >&2; exit 1; }
test -f "$operond_bin" || { echo "missing binary: $operond_bin" >&2; exit 1; }
if [[ "$(uname -s)" == "Darwin" ]]; then
  test -f "$archive_dir/libfuse-t.dylib" || {
    echo "missing bundled macOS FUSE-T runtime library: $archive_dir/libfuse-t.dylib" >&2
    exit 1
  }
fi

"$operon_bin" --version
"$operond_bin" --version
"$operon_bin" --help >/dev/null
"$operon_bin" doctor --help >/dev/null
"$operon_bin" exec --help >/dev/null

tar -tzf "$WORKDIR/assets/operon-sdk-js-${TAG}.tar.gz" \
  | grep -E '(^|/)dist/' >/dev/null
tar -tzf "$WORKDIR/assets/operon-sdk-js-${TAG}.tar.gz" \
  | grep -E '(^|/)generated/' >/dev/null

echo "release artifact verification passed for $REPO@$TAG on $asset"
