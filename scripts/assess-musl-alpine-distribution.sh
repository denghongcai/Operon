#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/assess-musl-alpine-distribution.sh <tag> [owner/repo]
  scripts/assess-musl-alpine-distribution.sh --dry-run <tag> [owner/repo]

Runs the current public glibc Linux release archive in an Alpine/musl
container and confirms the prebuilt archive is unsupported there.
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

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${OPERON_MUSL_ALPINE_IMAGE:-alpine:3.20}"

if [[ "$DRY_RUN" == true ]]; then
  echo "repo=$REPO"
  echo "tag=$TAG"
  echo "image=$image"
  echo "expected=unsupported-glibc-archive-on-musl"
  exit 0
fi

container_runtime="${OPERON_CONTAINER_RUNTIME:-}"
if [[ -z "$container_runtime" ]]; then
  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    container_runtime=docker
  elif command -v podman >/dev/null 2>&1; then
    container_runtime=podman
  else
    echo "docker or podman is required for musl/Alpine distribution assessment" >&2
    exit 1
  fi
fi

command -v "$container_runtime" >/dev/null || {
  echo "container runtime not found: $container_runtime" >&2
  exit 1
}

echo "running musl/Alpine distribution assessment in $image with $container_runtime"
"$container_runtime" run --rm \
  -e OPERON_RELEASE_INSTALL_WORKDIR=/tmp/operon-musl-assessment \
  -v "$ROOT:/workspace:ro" \
  "$image" \
  sh -lc '
    set -eu
    apk add --no-cache bash ca-certificates coreutils curl tar unzip >/dev/null
    cd /workspace
    bash -lc '"'"'
      set -euo pipefail
      source scripts/lib/release-install.sh
      tag="$0"
      repo="$1"
      asset="$(release_install_current_asset_name "$tag")"
      workdir="${OPERON_RELEASE_INSTALL_WORKDIR:-/tmp/operon-musl-assessment}"
      assets_dir="$workdir/assets"
      extract_dir="$workdir/extracted"
      prefix="$workdir/prefix"
      mkdir -p "$assets_dir" "$extract_dir" "$prefix/bin"
      release_url="https://github.com/${repo}/releases/download/${tag}"
      curl -fsSL "$release_url/SHA256SUMS" -o "$assets_dir/SHA256SUMS"
      curl -fsSL "$release_url/$asset" -o "$assets_dir/$asset"
      grep -E "[ *]${asset}$" "$assets_dir/SHA256SUMS" >"$assets_dir/SHA256SUMS.current"
      (cd "$assets_dir" && sha256sum -c SHA256SUMS.current)
      set +e
      tar -xzf "$assets_dir/$asset" -C "$extract_dir"
      tar_status=$?
      set -e
      if [[ "$tar_status" -ne 0 ]]; then
        echo "tar reported status $tar_status during Alpine assessment; continuing if binaries were extracted"
      fi
      archive_dir="$extract_dir/${asset%.tar.gz}"
      test -f "$archive_dir/operon" || { echo "missing extracted operon binary in $archive_dir" >&2; exit 1; }
      test -f "$archive_dir/operond" || { echo "missing extracted operond binary in $archive_dir" >&2; exit 1; }
      cp "$archive_dir/operon" "$prefix/bin/operon"
      cp "$archive_dir/operond" "$prefix/bin/operond"
      chmod +x "$prefix/bin/operon" "$prefix/bin/operond" 2>/dev/null || true
      export PATH="$prefix/bin:$PATH"
      echo "PATH points at isolated install prefix: $prefix/bin"
      set +e
      output="$(operon --version 2>&1)"
      status=$?
      set -e
      echo "alpine_status=$status"
      printf "%s\n" "$output"
      if [[ "$status" -eq 0 ]]; then
        echo "expected glibc Linux archive to be unsupported on Alpine/musl, but operon --version succeeded" >&2
        exit 1
      fi
      if ! grep -Eiq "not found|No such file|ld-linux|glibc|musl|interpreter" <<<"$output"; then
        echo "Alpine failure did not look like a glibc/musl loader boundary" >&2
        exit 1
      fi
      echo "musl/Alpine assessment captured expected unsupported glibc archive behavior"
    '"'"' "$0" "$1"
  ' "$TAG" "$REPO"

echo "musl/Alpine distribution assessment passed for $REPO@$TAG"
