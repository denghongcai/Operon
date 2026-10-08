#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
source scripts/lib/validation.sh
source scripts/lib/release-assets.sh
for file in scripts/lib/release-assets.sh scripts/lib/release-install.sh scripts/verify-alpine-release.sh \
  scripts/verify-release-artifacts.sh scripts/verify-release-gates.sh scripts/release-gate-orchestrate.sh; do
  bash -n "$file"
done
require_pattern 'build-rust-musl' .github/workflows/release-draft.yml
require_pattern 'release_archive_only: true' .github/workflows/release-draft.yml
require_pattern 'workflow_call' .github/workflows/alpine-musl-acceptance.yml
require_pattern 'Verify Alpine Release' scripts/release-gate-orchestrate.sh .github/workflows/verify-alpine-release.yml
require_pattern 'Native rebooted Alpine OpenRC service acceptance' scripts/verify-release-gates.sh
require_pattern 'ubuntu-24.04-arm' .github/workflows/verify-alpine-release.yml
[[ "$(release_expected_assets v0.16.11 | wc -l)" == 8 ]]
[[ "$(release_expected_assets v0.16.12 | wc -l)" == 10 ]]
[[ "$(release_expected_assets v0.17.0 | wc -l)" == 10 ]]
[[ "$(release_expected_assets v1.0.0 | wc -l)" == 10 ]]
[[ "$(release_expected_assets v0.13 | wc -l)" == 8 ]]
release_expected_assets v0.16.12 | grep -Fxq operon-v0.16.12-linux-musl-arm64.tar.gz
if release_validate_tag 'vgarbage'; then echo 'invalid tag accepted' >&2; exit 1; fi
[[ "$(release_resolve_linux_libc true false)" == gnu ]]
[[ "$(release_resolve_linux_libc false true)" == musl ]]
if release_resolve_linux_libc false false; then echo 'unknown libc accepted' >&2; exit 1; fi
if release_resolve_linux_libc true true; then echo 'ambiguous libc accepted' >&2; exit 1; fi
(
  uname() { case "$1" in -s) echo Linux ;; -m) echo x86_64 ;; esac; }
  [[ "$(OPERON_RELEASE_LIBC=gnu release_current_asset_name v0.16.12)" == operon-v0.16.12-linux-x86_64.tar.gz ]]
  [[ "$(OPERON_RELEASE_LIBC=musl release_current_asset_name v0.16.12)" == operon-v0.16.12-linux-musl-x86_64.tar.gz ]]
  if OPERON_RELEASE_LIBC=musl release_current_asset_name v0.16.11; then echo 'historical musl archive invented' >&2; exit 1; fi
  if OPERON_RELEASE_LIBC=bad release_current_asset_name v0.16.12; then echo 'bad override accepted' >&2; exit 1; fi
  uname() { case "$1" in -s) echo Linux ;; -m) echo aarch64 ;; esac; }
  [[ "$(OPERON_RELEASE_LIBC=musl release_current_asset_name v0.16.12)" == operon-v0.16.12-linux-musl-arm64.tar.gz ]]
  uname() { case "$1" in -s) echo Linux ;; -m) echo armv7l ;; esac; }
  [[ "$(OPERON_RELEASE_LIBC=gnu release_current_asset_name v0.16.12)" == operon-v0.16.12-linux-armv7.tar.gz ]]
  if OPERON_RELEASE_LIBC=musl release_current_asset_name v0.16.12; then echo 'unsupported ARMv7 musl accepted' >&2; exit 1; fi
  uname() { case "$1" in -s) echo Darwin ;; -m) echo arm64 ;; esac; }
  [[ "$(release_current_asset_name v0.16.12)" == operon-v0.16.12-macos-aarch64.tar.gz ]]
  uname() { case "$1" in -s) echo MINGW64_NT-10.0 ;; -m) echo x86_64 ;; esac; }
  [[ "$(release_current_asset_name v0.16.12)" == operon-v0.16.12-windows-x86_64.zip ]]
)
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64|Linux-aarch64|Linux-arm64)
    OPERON_RELEASE_LIBC=musl bash scripts/verify-release-install-usability.sh --dry-run v0.16.12 denghongcai/Operon | grep -Fq 'asset=operon-v0.16.12-linux-musl-'
    ;;
esac
OPERON_RELEASE_LIBC=gnu bash scripts/verify-release-artifacts.sh --dry-run v0.16.11 denghongcai/Operon >/dev/null
echo 'versioned assets and libc-aware musl release integration contracts passed'
