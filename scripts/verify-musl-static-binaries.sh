#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 3 ]]; then
  echo "usage: $0 <x86_64-unknown-linux-musl|aarch64-unknown-linux-musl> <operon> <operond>" >&2
  exit 2
fi
case "$1" in
  x86_64-unknown-linux-musl) machine='Advanced Micro Devices X86-64' ;;
  aarch64-unknown-linux-musl) machine='AArch64' ;;
  *) echo "unsupported musl target: $1" >&2; exit 2 ;;
esac
command -v readelf >/dev/null || { echo 'readelf is required' >&2; exit 1; }
for binary in "$2" "$3"; do
  [[ -f "$binary" && -x "$binary" ]] || { echo "missing executable: $binary" >&2; exit 1; }
  header="$(readelf -h "$binary")"
  actual_machine="$(awk -F: '/Machine:/ {sub(/^[[:space:]]+/, "", $2); print $2}' <<<"$header")"
  [[ "$actual_machine" == "$machine" ]] || {
    echo "wrong ELF architecture for $binary: $actual_machine (expected $machine)" >&2; exit 1;
  }
  program="$(readelf -l "$binary")"
  dynamic="$(readelf -d "$binary")"
  versions="$(readelf --version-info "$binary")"
  if [[ "$program" == *INTERP* || "$program" == *'Requesting program interpreter'* ]]; then
    echo "static contract violated: $binary has a dynamic interpreter" >&2; exit 1
  fi
  if [[ "$dynamic" == *'(NEEDED)'* || "$versions" == *GLIBC_* ]]; then
    echo "static contract violated: $binary requires dynamic libraries or GLIBC symbols" >&2; exit 1
  fi
  echo "static ELF contract passed: $binary ($1)"
done
