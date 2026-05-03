#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

require_file() {
  local path="$1"
  test -f "$path" || {
    echo "missing required file: $path" >&2
    exit 1
  }
}

require_pattern() {
  local pattern="$1"
  local path="$2"
  grep -Eq "$pattern" "$path" || {
    echo "missing required pattern in $path: $pattern" >&2
    exit 1
  }
}

reject_pattern() {
  local pattern="$1"
  local path="$2"
  if grep -En "$pattern" "$path"; then
    echo "unexpected pattern in $path: $pattern" >&2
    exit 1
  fi
}

require_file .github/workflows/release-draft.yml
require_file README.md

require_pattern 'image: ubuntu:20\.04' .github/workflows/release-draft.yml
require_pattern 'PROTOC_VERSION: "25\.3"' .github/workflows/release-draft.yml
require_pattern 'glibc 2\.31' README.md
reject_pattern 'glibc 2\.39' README.md

if [[ $# -gt 0 ]]; then
  for binary in "$@"; do
    test -x "$binary" || {
      echo "release binary is not executable: $binary" >&2
      exit 1
    }
    if readelf --version-info "$binary" | grep -E 'GLIBC_2\.([3-9][2-9]|[4-9][0-9])' >&2; then
      echo "release binary requires a GLIBC version newer than 2.31: $binary" >&2
      exit 1
    fi
  done
fi

echo "release glibc baseline validation passed"
