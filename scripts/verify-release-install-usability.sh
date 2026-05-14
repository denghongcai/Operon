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
    *) echo "unsupported release install platform: ${system}-${machine}" >&2; return 1 ;;
  esac
}

asset="$(current_asset_name "$TAG")"

if [[ "$DRY_RUN" == true ]]; then
  echo "repo=$REPO"
  echo "tag=$TAG"
  echo "asset=$asset"
  echo "install_prefix=\${OPERON_RELEASE_INSTALL_PREFIX:-temporary-prefix}"
  echo "PATH points at isolated install prefix"
  echo "operon doctor --mount-runtime"
  exit 0
fi

command -v curl >/dev/null || {
  echo "curl is required to download release install assets" >&2
  exit 1
}
command -v sha256sum >/dev/null || {
  echo "sha256sum is required to verify release install assets" >&2
  exit 1
}

workdir="${OPERON_RELEASE_INSTALL_WORKDIR:-$(mktemp -d)}"
cleanup() {
  if [[ -n "${daemon_pid:-}" ]]; then
    kill "$daemon_pid" >/dev/null 2>&1 || true
    wait "$daemon_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$workdir"
}
trap cleanup EXIT

assets_dir="$workdir/assets"
extract_dir="$workdir/extracted"
prefix="${OPERON_RELEASE_INSTALL_PREFIX:-$workdir/prefix}"
home_dir="$workdir/home"
mkdir -p "$assets_dir" "$extract_dir" "$prefix/bin" "$home_dir"

release_url="https://github.com/${REPO}/releases/download/${TAG}"
curl -fsSL "$release_url/SHA256SUMS" -o "$assets_dir/SHA256SUMS"
curl -fsSL "$release_url/$asset" -o "$assets_dir/$asset"

grep -E "[ *]${asset}$" "$assets_dir/SHA256SUMS" >"$assets_dir/SHA256SUMS.current" || {
  echo "SHA256SUMS does not contain $asset" >&2
  exit 1
}
(
  cd "$assets_dir"
  sha256sum -c SHA256SUMS.current
)

suffix=""
case "$asset" in
  *.zip)
    command -v unzip >/dev/null || {
      echo "unzip is required to verify Windows release install archives" >&2
      exit 1
    }
    unzip -q "$assets_dir/$asset" -d "$extract_dir"
    suffix=".exe"
    archive_dir="$extract_dir/${asset%.zip}"
    ;;
  *.tar.gz)
    tar -xzf "$assets_dir/$asset" -C "$extract_dir"
    archive_dir="$extract_dir/${asset%.tar.gz}"
    ;;
  *)
    echo "unsupported release install archive format: $asset" >&2
    exit 1
    ;;
esac

test -f "$archive_dir/operon$suffix" || { echo "missing operon binary in $archive_dir" >&2; exit 1; }
test -f "$archive_dir/operond$suffix" || { echo "missing operond binary in $archive_dir" >&2; exit 1; }

cp "$archive_dir/operon$suffix" "$prefix/bin/operon$suffix"
cp "$archive_dir/operond$suffix" "$prefix/bin/operond$suffix"
if [[ -f "$archive_dir/libfuse-t.dylib" ]]; then
  cp "$archive_dir/libfuse-t.dylib" "$prefix/bin/libfuse-t.dylib"
fi
find "$archive_dir" -maxdepth 1 -type f -name '*.dll' -exec cp {} "$prefix/bin/" \;
chmod +x "$prefix/bin/operon$suffix" "$prefix/bin/operond$suffix" 2>/dev/null || true

export PATH="$prefix/bin:$PATH"
export HOME="$home_dir"

resolved_operon="$(command -v operon || command -v "operon$suffix")"
resolved_operond="$(command -v operond || command -v "operond$suffix")"
prefix_bin="$(cd "$prefix/bin" && pwd -P)"
operon_dir="$(cd "$(dirname "$resolved_operon")" && pwd -P)"
operond_dir="$(cd "$(dirname "$resolved_operond")" && pwd -P)"
if [[ "$operon_dir" != "$prefix_bin" || "$operond_dir" != "$prefix_bin" ]]; then
  echo "PATH does not point at isolated install prefix" >&2
  echo "operon=$resolved_operon" >&2
  echo "operond=$resolved_operond" >&2
  echo "prefix=$prefix_bin" >&2
  exit 1
fi
echo "PATH points at isolated install prefix: $prefix_bin"

operon --version
operond --version
operon --help >/dev/null
operond --help >/dev/null
operon doctor --help >/dev/null
operon doctor --mount-runtime >/dev/null

operon init config "$workdir/init-config.yaml" >/dev/null

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

operond start --config "$HOME/.operon/config.yaml" >"$workdir/operond.log" 2>&1 &
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
  sed -n '1,120p' "$workdir/operond.log" >&2 || true
  exit 1
fi

operon node ping local >/dev/null
operon capability list local >/dev/null
operon doctor --node local >/dev/null
operon fs write local:/install-smoke.txt --content "hello from installed Operon" >/dev/null
test "$(operon fs read local:/install-smoke.txt)" = "hello from installed Operon"
operon audit show local --limit 10 >/dev/null

echo "release install usability verification passed for $REPO@$TAG on $asset"
