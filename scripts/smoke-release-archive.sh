#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage:
  scripts/smoke-release-archive.sh [--no-run] <archive>

Extracts one Operon release archive, verifies the packaged file contract, and
optionally smoke-runs the packaged binaries from the extracted archive.
USAGE
}

run_binaries=true
if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "${1:-}" == "--no-run" ]]; then
  run_binaries=false
  shift
fi

archive="${1:-}"
if [[ -z "$archive" ]]; then
  usage >&2
  exit 2
fi

if [[ ! -f "$archive" ]]; then
  echo "release archive does not exist: $archive" >&2
  exit 1
fi

case "$(basename "$archive")" in
  operon-v*-linux-*.tar.gz|operon-v*-macos-*.tar.gz|operon-v*-windows-*.zip) ;;
  *)
    echo "unsupported Operon release archive name: $(basename "$archive")" >&2
    exit 1
    ;;
esac

workdir="$(mktemp -d)"
cleanup() {
  rm -rf "$workdir"
}
trap cleanup EXIT

mkdir -p "$workdir/extracted"
suffix=""
case "$archive" in
  *.zip)
    command -v unzip >/dev/null || {
      echo "unzip is required to verify Windows archives" >&2
      exit 1
    }
    unzip -q "$archive" -d "$workdir/extracted"
    suffix=".exe"
    archive_dir="$workdir/extracted/$(basename "${archive%.zip}")"
    ;;
  *.tar.gz)
    tar -xzf "$archive" -C "$workdir/extracted"
    archive_dir="$workdir/extracted/$(basename "${archive%.tar.gz}")"
    ;;
  *)
    echo "unsupported archive format: $archive" >&2
    exit 1
    ;;
esac

operon_bin="$archive_dir/operon${suffix}"
operond_bin="$archive_dir/operond${suffix}"

test -d "$archive_dir" || { echo "missing archive root: $archive_dir" >&2; exit 1; }
test -f "$archive_dir/README.md" || { echo "missing README.md in $archive_dir" >&2; exit 1; }
test -f "$archive_dir/PROTOCOL.md" || { echo "missing PROTOCOL.md in $archive_dir" >&2; exit 1; }
test -f "$operon_bin" || { echo "missing binary: $operon_bin" >&2; exit 1; }
test -f "$operond_bin" || { echo "missing binary: $operond_bin" >&2; exit 1; }

case "$(basename "$archive")" in
  operon-v*-macos-*.tar.gz)
    test -f "$archive_dir/libfuse-t.dylib" || {
      echo "missing bundled macOS FUSE-T runtime library: $archive_dir/libfuse-t.dylib" >&2
      exit 1
    }
    if command -v otool >/dev/null 2>&1; then
      otool -l "$operon_bin" | grep -Fq "@executable_path" || {
        echo "packaged macOS operon binary is missing @executable_path rpath" >&2
        exit 1
      }
    elif [[ "$(uname -s)" == "Darwin" ]]; then
      echo "otool is required to verify macOS release rpath on Darwin" >&2
      exit 1
    fi
    ;;
esac

if [[ "$run_binaries" != true ]]; then
  echo "release archive structure verification passed for $(basename "$archive")"
  exit 0
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
  env -u DYLD_LIBRARY_PATH \
    -u DYLD_FALLBACK_LIBRARY_PATH \
    -u DYLD_FRAMEWORK_PATH \
    "$operon_bin" --version
  env -u DYLD_LIBRARY_PATH \
    -u DYLD_FALLBACK_LIBRARY_PATH \
    -u DYLD_FRAMEWORK_PATH \
    "$operond_bin" --version
  env -u DYLD_LIBRARY_PATH \
    -u DYLD_FALLBACK_LIBRARY_PATH \
    -u DYLD_FRAMEWORK_PATH \
    "$operon_bin" --help >/dev/null
  env -u DYLD_LIBRARY_PATH \
    -u DYLD_FALLBACK_LIBRARY_PATH \
    -u DYLD_FRAMEWORK_PATH \
    "$operon_bin" doctor --help >/dev/null
  env -u DYLD_LIBRARY_PATH \
    -u DYLD_FALLBACK_LIBRARY_PATH \
    -u DYLD_FRAMEWORK_PATH \
    "$operon_bin" exec --help >/dev/null
else
  "$operon_bin" --version
  "$operond_bin" --version
  "$operon_bin" --help >/dev/null
  "$operon_bin" doctor --help >/dev/null
  "$operon_bin" exec --help >/dev/null
fi

echo "release archive smoke passed for $(basename "$archive")"
