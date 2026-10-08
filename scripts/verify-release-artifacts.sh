#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/release-assets.sh"

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
release_validate_tag "$TAG"

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
  release_expected_assets "$1"
}

current_asset_name() {
  release_current_asset_name "$1"
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

if [[ -n "${OPERON_RELEASE_VERIFY_DIR:-}" ]]; then
  mkdir -p "$OPERON_RELEASE_VERIFY_DIR"
  WORKDIR="$(mktemp -d "$OPERON_RELEASE_VERIFY_DIR/operon-artifacts.XXXXXX")"
else
  WORKDIR="$(mktemp -d)"
fi
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
    ;;
  *.tar.gz)
    ;;
  *)
    echo "unsupported archive format: $asset" >&2
    exit 1
    ;;
esac

scripts/smoke-release-archive.sh "$WORKDIR/assets/$asset"

tar -tzf "$WORKDIR/assets/operon-sdk-js-${TAG}.tar.gz" \
  | grep -E '(^|/)dist/' >/dev/null
tar -tzf "$WORKDIR/assets/operon-sdk-js-${TAG}.tar.gz" \
  | grep -E '(^|/)generated/' >/dev/null

echo "release artifact verification passed for $REPO@$TAG on $asset"
