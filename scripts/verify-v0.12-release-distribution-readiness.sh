#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.12-release-distribution-readiness.md
require_pattern 'Status: Completed' docs/plan/v0.12-release-distribution-readiness.md
require_pattern 'Phase 78: v0.12 Release / Distribution Readiness' docs/plan/development-phases.md
require_pattern 'No v0.12 work remains' docs/plan/development-phases.md

require_pattern 'build-rust-linux' .github/workflows/release-draft.yml
require_pattern 'build-rust-native' .github/workflows/release-draft.yml
require_pattern 'linux-x86_64' .github/workflows/release-draft.yml
require_pattern 'linux-arm64' .github/workflows/release-draft.yml
require_pattern 'linux-armv7' .github/workflows/release-draft.yml
require_pattern 'macos-x86_64' .github/workflows/release-draft.yml
require_pattern 'macos-aarch64' .github/workflows/release-draft.yml
require_pattern 'windows-x86_64' .github/workflows/release-draft.yml
require_pattern 'x86_64-apple-darwin' .github/workflows/release-draft.yml
require_pattern 'aarch64-apple-darwin' .github/workflows/release-draft.yml
require_pattern 'x86_64-pc-windows-msvc' .github/workflows/release-draft.yml
require_pattern 'macos-15-intel' .github/workflows/release-draft.yml
require_pattern 'macos-15' .github/workflows/release-draft.yml
require_pattern 'windows-latest' .github/workflows/release-draft.yml
require_pattern 'arduino/setup-protoc@v3' .github/workflows/release-draft.yml
require_pattern 'repo-token: \$\{\{ github\.token \}\}' .github/workflows/release-draft.yml
require_pattern 'choco install winfsp -y' .github/workflows/release-draft.yml
require_pattern 'operon.*--version' .github/workflows/release-draft.yml
require_pattern 'operond.*--version' .github/workflows/release-draft.yml
require_pattern 'doctor --help' .github/workflows/release-draft.yml
require_pattern 'exec --help' .github/workflows/release-draft.yml
require_pattern 'dist/\*\.tar\.gz' .github/workflows/release-draft.yml
require_pattern 'dist/\*\.zip' .github/workflows/release-draft.yml
require_pattern 'sha256sum \*\.tar\.gz \*\.zip > SHA256SUMS' .github/workflows/release-draft.yml

require_pattern 'Linux and macOS release archives use' README.md
require_pattern '`.tar.gz`; Windows release archives use `.zip`' README.md
require_pattern 'Windows release archives use `.zip`' README.md
require_pattern 'macOS FUSE-T' README.md
require_pattern 'Windows WinFsp' README.md
require_pattern 'prebuilt archives include' README.md
require_pattern 'platform live mount' README.md
require_pattern 'macos-x86_64' docs/architecture/technology-and-protocol-decisions.md
require_pattern 'macos-aarch64' docs/architecture/technology-and-protocol-decisions.md
require_pattern 'windows-x86_64' docs/architecture/technology-and-protocol-decisions.md
require_pattern 'core runtime preview' docs/architecture/technology-and-protocol-decisions.md

require_pattern 'macos-x86_64' scripts/verify-readme-quickstart-docker.sh

echo "v0.12 release distribution readiness validation passed"
