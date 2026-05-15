#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/verify-release-install-usability.sh <tag> [owner/repo]
  scripts/verify-release-install-usability.sh --dry-run <tag> [owner/repo]

Downloads the current platform's public Operon release archive, verifies its
checksum, installs operon and operond into an isolated prefix, proves PATH uses
that prefix, and runs first-use local daemon smoke checks.
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

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/release-install.sh
source "$ROOT/scripts/lib/release-install.sh"

TAG="${1:-}"
REPO="${2:-${GITHUB_REPOSITORY:-}}"

if [[ -z "$TAG" ]]; then
  usage
  exit 1
fi

REPO="$(release_install_repo_from_remote "$REPO")"

if [[ -z "$REPO" ]]; then
  echo "failed to determine GitHub repository; pass owner/repo explicitly" >&2
  exit 1
fi

asset="$(release_install_current_asset_name "$TAG")"

if [[ "$DRY_RUN" == true ]]; then
  echo "repo=$REPO"
  echo "tag=$TAG"
  echo "asset=$asset"
  echo "install_prefix=\${OPERON_RELEASE_INSTALL_PREFIX:-temporary-prefix}"
  echo "PATH points at isolated install prefix"
  echo "operon doctor --mount-runtime"
  exit 0
fi

cleanup() {
  if [[ -n "${daemon_pid:-}" ]]; then
    kill "$daemon_pid" >/dev/null 2>&1 || true
    wait "$daemon_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$RELEASE_INSTALL_WORKDIR"
}
trap cleanup EXIT

release_install_setup "$TAG" "$REPO"

operon --version
operond --version
operon --help >/dev/null
operond --help >/dev/null
operon doctor --help >/dev/null
operon doctor --mount-runtime >/dev/null

operon init config "$RELEASE_INSTALL_WORKDIR/init-config.yaml" >/dev/null

pick_port() {
  if command -v python3 >/dev/null 2>&1; then
    python3 - <<'PY'
import socket
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
    return
  fi
  if command -v python >/dev/null 2>&1; then
    python - <<'PY'
import socket
sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
    return
  fi
  echo 17789
}

port="$(pick_port)"
workspace="$HOME/operon-workspace"
mkdir -p "$workspace" "$HOME/.operon"
operon onboard \
  --yes \
  --role both \
  --output-dir "$HOME/.operon" \
  --node-id local \
  --workspace "$workspace" \
  --listen "127.0.0.1:$port" \
  >/dev/null

operond start --config "$HOME/.operon/config.yaml" >"$RELEASE_INSTALL_WORKDIR/operond.log" 2>&1 &
daemon_pid=$!

node_ready=false
for _ in $(seq 1 30); do
  if operon node ping local >/dev/null 2>&1; then
    node_ready=true
    break
  fi
  sleep 1
done

if [[ "$node_ready" != true ]]; then
  echo "installed daemon did not become ready" >&2
  sed -n '1,120p' "$RELEASE_INSTALL_WORKDIR/operond.log" >&2 || true
  exit 1
fi

operon node ping local >/dev/null
operon capability list local >/dev/null
operon doctor --node local >/dev/null
operon fs write local:/install-smoke.txt --content "hello from installed Operon" >/dev/null
test "$(operon fs read local:/install-smoke.txt)" = "hello from installed Operon"
operon audit show local --limit 10 >/dev/null

echo "release install usability verification passed for $REPO@$TAG on $asset"
