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
require_file scripts/smoke-release-archive.sh
require_pattern 'gh release download' scripts/verify-release-artifacts.sh
require_pattern 'sha256sum -c SHA256SUMS' scripts/verify-release-artifacts.sh
require_pattern 'release_expected_assets' scripts/verify-release-artifacts.sh
require_pattern 'operon-\$\{tag\}-linux-x86_64\.tar\.gz' scripts/lib/release-assets.sh
require_pattern 'operon-\$\{tag\}-macos-aarch64\.tar\.gz' scripts/lib/release-assets.sh
require_pattern 'operon-\$\{tag\}-windows-x86_64\.zip' scripts/lib/release-assets.sh
# Verify the historical behavior, not its former heredoc implementation.
source "$ROOT/scripts/lib/release-assets.sh"
release_expected_assets v0.13.1 | grep -Fxq 'operon-sdk-js-v0.13.1.tar.gz'
if release_expected_assets v0.16.16 | grep -q 'sdk'; then
  echo 'future SDK assets must be distributed through npm' >&2; exit 1
fi
require_pattern 'scripts/smoke-release-archive.sh "\$WORKDIR/assets/\$asset"' scripts/verify-release-artifacts.sh
require_pattern 'doctor --help' scripts/smoke-release-archive.sh
require_pattern 'exec --help' scripts/smoke-release-archive.sh

require_file .github/workflows/verify-release-artifacts.yml
require_file .github/workflows/verify-readme-quickstart.yml
require_pattern 'workflow_dispatch' .github/workflows/verify-release-artifacts.yml
require_pattern 'ubuntu-24.04' .github/workflows/verify-release-artifacts.yml
require_pattern 'ubuntu-24.04-arm' .github/workflows/verify-release-artifacts.yml
require_pattern 'macos-15' .github/workflows/verify-release-artifacts.yml
require_pattern 'macos-15-intel' .github/workflows/verify-release-artifacts.yml
require_pattern 'windows-2025' .github/workflows/verify-release-artifacts.yml
require_pattern 'scripts/verify-release-artifacts.sh' .github/workflows/verify-release-artifacts.yml
require_pattern 'workflow_dispatch' .github/workflows/verify-readme-quickstart.yml
require_pattern 'OPERON_VERSION: \$\{\{ inputs.tag \}\}' .github/workflows/verify-readme-quickstart.yml
require_pattern 'scripts/verify-readme-quickstart-docker.sh' .github/workflows/verify-readme-quickstart.yml

require_pattern 'Verify README Quickstart' DEVELOPMENT.md
require_pattern 'Do not substitute local runs for release-completion evidence' DEVELOPMENT.md
require_pattern 'Verify Release Artifacts' DEVELOPMENT.md
require_pattern 'README Quickstart' docs/plan/v0.12.4-release-artifact-verification.md
require_pattern 'v0.12.4 Release Artifact Verification Validation' scripts/ci/run-validations.sh

bash -n scripts/verify-release-artifacts.sh
bash -n scripts/smoke-release-archive.sh
scripts/verify-release-artifacts.sh --dry-run v0.13.1 >/dev/null

echo "v0.12.4 release artifact verification validation passed"
