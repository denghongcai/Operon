#!/usr/bin/env bash
set -euo pipefail
[[ $# == 2 ]] || { echo "usage: $0 <native-gnu-bin-directory> <native-musl-bin-directory>" >&2; exit 2; }
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
gnu_directory="$(realpath "$1")"
musl_directory="$(realpath "$2")"
test -c /dev/fuse || { echo 'required native FUSE device is missing' >&2; exit 1; }
fixture="$(mktemp -d /tmp/operon-libc-comparison.XXXXXX)"
trap 'rm -rf "$fixture"' EXIT
mkdir "$fixture/gnu" "$fixture/musl"
for binary in operon operond; do
  cp "$gnu_directory/$binary" "$fixture/gnu/$binary"
  cp "$musl_directory/$binary" "$fixture/musl/$binary"
done
docker run --rm --device /dev/fuse --cap-add SYS_ADMIN --security-opt apparmor=unconfined \
  --mount "type=bind,src=$ROOT,dst=/workspace,readonly" \
  --mount "type=bind,src=$fixture/gnu,dst=/gnu,readonly" \
  --mount "type=bind,src=$fixture/musl,dst=/musl,readonly" \
  -e OPERON_LIBC_BENCH_TIMEOUT_SECS="${OPERON_LIBC_BENCH_TIMEOUT_SECS:-120}" \
  -e OPERON_LIBC_BENCH_STOP_TIMEOUT_SECS="${OPERON_LIBC_BENCH_STOP_TIMEOUT_SECS:-30}" \
  -e OPERON_LIBC_BENCH_OPERATIONS="${OPERON_LIBC_BENCH_OPERATIONS:-200}" \
  -e OPERON_LIBC_BENCH_EXEC_OPERATIONS="${OPERON_LIBC_BENCH_EXEC_OPERATIONS:-30}" \
  -e OPERON_LIBC_BENCH_REPEATS="${OPERON_LIBC_BENCH_REPEATS:-3}" \
  ubuntu@sha256:c4a8d5503dfb2a3eb8ab5f807da5bc69a85730fb49b5cfca2330194ebcc41c7b /bin/sh -ec '
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends python3 fuse3 time >/dev/null
    python3 /workspace/scripts/performance/libc-runtime.py --gnu-bin-dir /gnu --musl-bin-dir /musl \
      --operations "$OPERON_LIBC_BENCH_OPERATIONS" --exec-operations "$OPERON_LIBC_BENCH_EXEC_OPERATIONS" --repeats "$OPERON_LIBC_BENCH_REPEATS"
  '
