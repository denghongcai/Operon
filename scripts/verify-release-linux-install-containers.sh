#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/verify-release-linux-install-containers.sh <tag> [owner/repo]
  scripts/verify-release-linux-install-containers.sh --dry-run <tag> [owner/repo]

Runs release install usability smoke inside Linux distribution containers.
By default this covers ubuntu:20.04 for the documented glibc baseline and
debian:12 as a current stable distribution image. Set
OPERON_CONTAINER_RUNTIME=docker or OPERON_CONTAINER_RUNTIME=podman to choose a
specific local container runtime.
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
images="${OPERON_RELEASE_INSTALL_LINUX_IMAGES:-ubuntu:20.04 debian:12}"

if [[ "$DRY_RUN" == true ]]; then
  echo "repo=$REPO"
  echo "tag=$TAG"
  for image in $images; do
    echo "image=$image"
  done
  exit 0
fi

container_runtime="${OPERON_CONTAINER_RUNTIME:-}"
if [[ -z "$container_runtime" ]]; then
  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    container_runtime=docker
  elif command -v podman >/dev/null 2>&1; then
    container_runtime=podman
  else
    echo "docker or podman is required for Linux release install container smoke" >&2
    exit 1
  fi
fi

command -v "$container_runtime" >/dev/null || {
  echo "container runtime not found: $container_runtime" >&2
  exit 1
}

for image in $images; do
  echo "running release install usability smoke in $image with $container_runtime"
  "$container_runtime" run --rm \
    -e DEBIAN_FRONTEND=noninteractive \
    -e OPERON_RELEASE_INSTALL_WORKDIR=/tmp/operon-release-install \
    -v "$ROOT:/workspace:ro" \
    "$image" \
    bash -lc '
      set -euo pipefail
      apt-get update >/dev/null
      apt-get install -y --no-install-recommends \
        ca-certificates \
        coreutils \
        curl \
        procps \
        tar \
        unzip \
        python3 \
        >/dev/null
      cd /workspace
      scripts/verify-release-install-usability.sh "$0" "$1"
    ' "$TAG" "$REPO"
done

echo "Linux release install container smoke passed for $REPO@$TAG"
