#!/usr/bin/env bash

release_install_current_asset_name() {
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

release_install_repo_from_remote() {
  local repo="${1:-}"
  if [[ -n "$repo" ]]; then
    printf '%s\n' "$repo"
    return
  fi

  if [[ -n "${GITHUB_REPOSITORY:-}" ]]; then
    printf '%s\n' "$GITHUB_REPOSITORY"
    return
  fi

  local remote_url
  if remote_url="$(git remote get-url origin 2>/dev/null)"; then
    printf '%s\n' "$remote_url" \
      | sed -E 's#^git@github.com:##; s#^https://github.com/##; s#\.git$##'
  fi
}

release_install_setup() {
  local tag="$1"
  local repo="$2"

  command -v curl >/dev/null || {
    echo "curl is required to download release install assets" >&2
    return 1
  }
  command -v sha256sum >/dev/null || {
    echo "sha256sum is required to verify release install assets" >&2
    return 1
  }

  RELEASE_INSTALL_ASSET="$(release_install_current_asset_name "$tag")"
  RELEASE_INSTALL_WORKDIR="${OPERON_RELEASE_INSTALL_WORKDIR:-$(mktemp -d)}"
  RELEASE_INSTALL_ASSETS_DIR="$RELEASE_INSTALL_WORKDIR/assets"
  RELEASE_INSTALL_EXTRACT_DIR="$RELEASE_INSTALL_WORKDIR/extracted"
  RELEASE_INSTALL_PREFIX="${OPERON_RELEASE_INSTALL_PREFIX:-$RELEASE_INSTALL_WORKDIR/prefix}"
  RELEASE_INSTALL_HOME="$RELEASE_INSTALL_WORKDIR/home"
  mkdir -p \
    "$RELEASE_INSTALL_ASSETS_DIR" \
    "$RELEASE_INSTALL_EXTRACT_DIR" \
    "$RELEASE_INSTALL_PREFIX/bin" \
    "$RELEASE_INSTALL_HOME"

  local release_url="https://github.com/${repo}/releases/download/${tag}"
  curl -fsSL "$release_url/SHA256SUMS" -o "$RELEASE_INSTALL_ASSETS_DIR/SHA256SUMS"
  curl -fsSL "$release_url/$RELEASE_INSTALL_ASSET" -o "$RELEASE_INSTALL_ASSETS_DIR/$RELEASE_INSTALL_ASSET"

  grep -E "[ *]${RELEASE_INSTALL_ASSET}$" \
    "$RELEASE_INSTALL_ASSETS_DIR/SHA256SUMS" \
    >"$RELEASE_INSTALL_ASSETS_DIR/SHA256SUMS.current" || {
      echo "SHA256SUMS does not contain $RELEASE_INSTALL_ASSET" >&2
      return 1
    }
  (
    cd "$RELEASE_INSTALL_ASSETS_DIR"
    sha256sum -c SHA256SUMS.current
  )

  RELEASE_INSTALL_SUFFIX=""
  case "$RELEASE_INSTALL_ASSET" in
    *.zip)
      command -v unzip >/dev/null || {
        echo "unzip is required to verify Windows release install archives" >&2
        return 1
      }
      unzip -q "$RELEASE_INSTALL_ASSETS_DIR/$RELEASE_INSTALL_ASSET" -d "$RELEASE_INSTALL_EXTRACT_DIR"
      RELEASE_INSTALL_SUFFIX=".exe"
      RELEASE_INSTALL_ARCHIVE_DIR="$RELEASE_INSTALL_EXTRACT_DIR/${RELEASE_INSTALL_ASSET%.zip}"
      ;;
    *.tar.gz)
      tar -xzf "$RELEASE_INSTALL_ASSETS_DIR/$RELEASE_INSTALL_ASSET" -C "$RELEASE_INSTALL_EXTRACT_DIR"
      RELEASE_INSTALL_ARCHIVE_DIR="$RELEASE_INSTALL_EXTRACT_DIR/${RELEASE_INSTALL_ASSET%.tar.gz}"
      ;;
    *)
      echo "unsupported release install archive format: $RELEASE_INSTALL_ASSET" >&2
      return 1
      ;;
  esac

  test -f "$RELEASE_INSTALL_ARCHIVE_DIR/operon$RELEASE_INSTALL_SUFFIX" || {
    echo "missing operon binary in $RELEASE_INSTALL_ARCHIVE_DIR" >&2
    return 1
  }
  test -f "$RELEASE_INSTALL_ARCHIVE_DIR/operond$RELEASE_INSTALL_SUFFIX" || {
    echo "missing operond binary in $RELEASE_INSTALL_ARCHIVE_DIR" >&2
    return 1
  }

  cp "$RELEASE_INSTALL_ARCHIVE_DIR/operon$RELEASE_INSTALL_SUFFIX" \
    "$RELEASE_INSTALL_PREFIX/bin/operon$RELEASE_INSTALL_SUFFIX"
  cp "$RELEASE_INSTALL_ARCHIVE_DIR/operond$RELEASE_INSTALL_SUFFIX" \
    "$RELEASE_INSTALL_PREFIX/bin/operond$RELEASE_INSTALL_SUFFIX"
  if [[ -f "$RELEASE_INSTALL_ARCHIVE_DIR/libfuse-t.dylib" ]]; then
    cp "$RELEASE_INSTALL_ARCHIVE_DIR/libfuse-t.dylib" "$RELEASE_INSTALL_PREFIX/bin/libfuse-t.dylib"
  fi
  find "$RELEASE_INSTALL_ARCHIVE_DIR" -maxdepth 1 -type f -name '*.dll' -exec cp {} "$RELEASE_INSTALL_PREFIX/bin/" \;
  chmod +x \
    "$RELEASE_INSTALL_PREFIX/bin/operon$RELEASE_INSTALL_SUFFIX" \
    "$RELEASE_INSTALL_PREFIX/bin/operond$RELEASE_INSTALL_SUFFIX" \
    2>/dev/null || true

  export PATH="$RELEASE_INSTALL_PREFIX/bin:$PATH"
  export HOME="$RELEASE_INSTALL_HOME"

  RELEASE_INSTALL_OPERON="$(command -v operon || command -v "operon$RELEASE_INSTALL_SUFFIX")"
  RELEASE_INSTALL_OPEROND="$(command -v operond || command -v "operond$RELEASE_INSTALL_SUFFIX")"
  RELEASE_INSTALL_PREFIX_BIN="$(cd "$RELEASE_INSTALL_PREFIX/bin" && pwd -P)"
  local operon_dir operond_dir
  operon_dir="$(cd "$(dirname "$RELEASE_INSTALL_OPERON")" && pwd -P)"
  operond_dir="$(cd "$(dirname "$RELEASE_INSTALL_OPEROND")" && pwd -P)"
  if [[ "$operon_dir" != "$RELEASE_INSTALL_PREFIX_BIN" || "$operond_dir" != "$RELEASE_INSTALL_PREFIX_BIN" ]]; then
    echo "PATH does not point at isolated install prefix" >&2
    echo "operon=$RELEASE_INSTALL_OPERON" >&2
    echo "operond=$RELEASE_INSTALL_OPEROND" >&2
    echo "prefix=$RELEASE_INSTALL_PREFIX_BIN" >&2
    return 1
  fi

  echo "PATH points at isolated install prefix: $RELEASE_INSTALL_PREFIX_BIN"
}
