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

if release_has_sdk_native_assets "$TAG"; then
  command -v node >/dev/null || { echo 'Node is required to verify native SDK manifests' >&2; exit 1; }
  while IFS= read -r sdk_target; do
    sdk_archive="$WORKDIR/assets/operon-sdk-native-${TAG}-${sdk_target}.tar.gz"
    entries=$(tar -tzf "$sdk_archive" | sort)
    [[ "$entries" == $'binding.json\noperon-sdk-native.node' ]] || { echo 'invalid SDK archive entries' >&2; exit 1; }
    tar -tvzf "$sdk_archive" | awk 'substr($1,1,1) != "-" { bad=1 } END { exit bad }'
    sdk_extract="$WORKDIR/extracted/$sdk_target"
    mkdir -p "$sdk_extract"
    tar -xzf "$sdk_archive" -C "$sdk_extract"
    node "$ROOT/scripts/verify-sdk-native-manifest.mjs" "$sdk_extract" "$(release_expected_version "$TAG")" "$sdk_target"
  done < <(release_sdk_native_targets)
  if [[ "$asset" == *-linux-armv7.tar.gz ]]; then
    echo 'ARMv7 daemon archive verified; native SDK is not supported on ARMv7'
  else
  case "$asset" in
    *-linux-x86_64.tar.gz) runtime_sdk_target=x86_64-unknown-linux-gnu ;;
    *-linux-arm64.tar.gz) runtime_sdk_target=aarch64-unknown-linux-gnu ;;
    *-linux-musl-x86_64.tar.gz) runtime_sdk_target=x86_64-unknown-linux-musl ;;
    *-linux-musl-arm64.tar.gz) runtime_sdk_target=aarch64-unknown-linux-musl ;;
    *-macos-x86_64.tar.gz) runtime_sdk_target=x86_64-apple-darwin ;;
    *-macos-aarch64.tar.gz) runtime_sdk_target=aarch64-apple-darwin ;;
    *-windows-x86_64.zip) runtime_sdk_target=x86_64-pc-windows-msvc ;;
    *) echo 'no native SDK runtime gate for this platform' >&2; exit 1 ;;
  esac
  mkdir -p "$WORKDIR/runtime"
  suffix=""
  if [[ "$asset" == *.zip ]]; then
    unzip -q "$WORKDIR/assets/$asset" -d "$WORKDIR/runtime"
    archive_root="${asset%.zip}"
    suffix=".exe"
  else
    tar -xzf "$WORKDIR/assets/$asset" -C "$WORKDIR/runtime"
    archive_root="${asset%.tar.gz}"
  fi
  tar -xzf "$WORKDIR/assets/operon-sdk-js-${TAG}.tar.gz" -C "$WORKDIR/runtime"
  sdk_root="$WORKDIR/runtime/operon-sdk-js-${TAG}"
  command -v npm >/dev/null || { echo 'npm is required for downloaded SDK acceptance' >&2; exit 1; }
  npm --prefix "$sdk_root" install --omit=dev --ignore-scripts --no-audit --no-fund
  runtime_daemon="$WORKDIR/runtime/$archive_root/operond$suffix"
  runtime_binding="$WORKDIR/extracted/$runtime_sdk_target/operon-sdk-native.node"
  runtime_js="$sdk_root/dist/index.js"
  if [[ "$suffix" == .exe ]]; then
    runtime_daemon="$(cygpath -w "$runtime_daemon")"
    runtime_binding="$(cygpath -w "$runtime_binding")"
    runtime_js="$(cygpath -w "$runtime_js")"
  fi
  OPERON_NATIVE_TEST_DAEMON="$runtime_daemon" OPERON_SDK_NATIVE_PATH="$runtime_binding" \
    OPERON_SDK_TEST_JS_PATH="$runtime_js" OPERON_SDK_TEST_PACKAGED=1 \
    node --expose-gc "$ROOT/scripts/verify-sdk-native.mjs"
  if [[ "$runtime_sdk_target" == *-unknown-linux-gnu ]]; then
    OPERON_NATIVE_TEST_DAEMON="$runtime_daemon" OPERON_SDK_NATIVE_PATH="$runtime_binding" \
      OPERON_SDK_TEST_JS_PATH="$runtime_js" OPERON_SDK_TEST_PACKAGED=1 \
      python3 "$ROOT/scripts/verify-packaged-sdk-relay.py"
  fi
  fi
fi

tar -tzf "$WORKDIR/assets/operon-sdk-js-${TAG}.tar.gz" \
  | grep -E '(^|/)dist/' >/dev/null
tar -tzf "$WORKDIR/assets/operon-sdk-js-${TAG}.tar.gz" \
  | grep -E '(^|/)generated/' >/dev/null

echo "release artifact verification passed for $REPO@$TAG on $asset"
