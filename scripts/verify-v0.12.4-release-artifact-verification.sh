#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.12.4-release-artifact-verification.md
require_pattern 'Status: Completed' docs/plan/v0.12.4-release-artifact-verification.md
require_pattern 'Phase 82: v0.12.4 Release Artifact Verification' docs/plan/development-phases.md
require_pattern 'No v0.12.4 work remains' docs/plan/development-phases.md

require_file scripts/verify-release-artifacts.sh
require_pattern 'gh release download' scripts/verify-release-artifacts.sh
require_pattern 'sha256sum -c SHA256SUMS' scripts/verify-release-artifacts.sh
require_pattern 'operon-\$\{tag\}-linux-x86_64\.tar\.gz' scripts/verify-release-artifacts.sh
require_pattern 'operon-\$\{tag\}-macos-aarch64\.tar\.gz' scripts/verify-release-artifacts.sh
require_pattern 'operon-\$\{tag\}-windows-x86_64\.zip' scripts/verify-release-artifacts.sh
require_pattern 'operon-sdk-js-\$\{tag\}\.tar\.gz' scripts/verify-release-artifacts.sh
require_pattern 'doctor --help' scripts/verify-release-artifacts.sh
require_pattern 'exec --help' scripts/verify-release-artifacts.sh

require_file .github/workflows/verify-release-artifacts.yml
require_pattern 'workflow_dispatch' .github/workflows/verify-release-artifacts.yml
require_pattern 'ubuntu-latest' .github/workflows/verify-release-artifacts.yml
require_pattern 'macos-latest' .github/workflows/verify-release-artifacts.yml
require_pattern 'windows-latest' .github/workflows/verify-release-artifacts.yml
require_pattern 'scripts/verify-release-artifacts.sh' .github/workflows/verify-release-artifacts.yml

require_pattern 'scripts/verify-release-artifacts.sh <tag>' DEVELOPMENT.md
require_pattern 'Verify Release Artifacts' DEVELOPMENT.md
require_pattern 'README Quickstart' docs/plan/v0.12.4-release-artifact-verification.md
require_pattern 'v0.12.4 Release Artifact Verification Validation' .github/workflows/ci.yml

bash -n scripts/verify-release-artifacts.sh
scripts/verify-release-artifacts.sh --dry-run v0.12.2 >/dev/null

echo "v0.12.4 release artifact verification validation passed"
