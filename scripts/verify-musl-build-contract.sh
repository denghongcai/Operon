#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
source scripts/lib/validation.sh
require_file docs/plan/v0.18.22-musl-alpine-distribution-roadmap.md
require_file scripts/build-musl-release.sh
require_file scripts/verify-musl-static-binaries.sh
require_pattern 'ubuntu-24.04-arm' .github/workflows/alpine-musl-acceptance.yml
require_pattern 'aarch64-unknown-linux-musl' .github/workflows/alpine-musl-acceptance.yml
require_pattern 'x86_64-unknown-linux-musl' .github/workflows/alpine-musl-acceptance.yml
require_pattern 'rust@sha256:9dfaae478ecd298b6b5a039e1f2cc4fc040fc818a2de9aa78fa714dea036574d' .github/workflows/alpine-musl-acceptance.yml
require_pattern 'musl-dev=1.2.5-r12' .github/workflows/alpine-musl-acceptance.yml
bash -n scripts/build-musl-release.sh scripts/verify-musl-static-binaries.sh
case "$(uname -m)" in
  x86_64) target=x86_64-unknown-linux-musl; other=aarch64-unknown-linux-musl ;;
  aarch64) target=aarch64-unknown-linux-musl; other=x86_64-unknown-linux-musl ;;
  *) echo 'unsupported verifier test architecture' >&2; exit 1 ;;
esac
task_dir="$(mktemp -d)"
trap 'rm -rf "$task_dir"' EXIT
# These C fixtures test ELF classification only; musl identity and actual
# runtime support are established by the pinned native build/Alpine gates.
cc -static scripts/fixtures/static-elf.c -o "$task_dir/static"
cc scripts/fixtures/static-elf.c -o "$task_dir/dynamic"
bash scripts/verify-musl-static-binaries.sh "$target" "$task_dir/static" "$task_dir/static"
if bash scripts/verify-musl-static-binaries.sh "$target" "$task_dir/dynamic" "$task_dir/static" >"$task_dir/dynamic.log" 2>&1; then
  echo 'dynamic fixture was incorrectly accepted' >&2; exit 1
fi
require_pattern 'dynamic interpreter|dynamic libraries' "$task_dir/dynamic.log"
if bash scripts/verify-musl-static-binaries.sh "$other" "$task_dir/static" "$task_dir/static" >"$task_dir/arch.log" 2>&1; then
  echo 'wrong architecture was incorrectly accepted' >&2; exit 1
fi
require_pattern 'wrong ELF architecture' "$task_dir/arch.log"
if bash scripts/verify-musl-static-binaries.sh "$target" "$task_dir/missing" "$task_dir/static" >"$task_dir/missing.log" 2>&1; then
  echo 'missing fixture was incorrectly accepted' >&2; exit 1
fi
require_pattern 'missing executable' "$task_dir/missing.log"
echo 'musl build and static verifier contract validation passed'
