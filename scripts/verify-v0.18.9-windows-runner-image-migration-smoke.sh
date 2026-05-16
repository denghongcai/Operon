#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.18.9-windows-runner-image-migration-smoke.md
require_pattern 'Status: Completed' docs/plan/v0.18.9-windows-runner-image-migration-smoke.md
require_pattern 'Phase 118: v0.18.9 Windows Runner Image Migration Smoke' docs/plan/development-phases.md
require_pattern 'No v0.18.9 Windows runner image migration smoke work remains' docs/plan/development-phases.md

require_file .github/workflows/windows-runner-image-smoke.yml
require_pattern 'name: Windows Runner Image Smoke' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'default: windows-2025' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'runs-on: \$\{\{ inputs\.runner_label \}\}' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'Get-CimInstance Win32_OperatingSystem' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'image.version=\$env:ImageVersion' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'choco install winfsp -y' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'cargo test -p operond --locked windows_job_object_cancellation_terminates_descendant_process' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'cargo build --release --locked --target x86_64-pc-windows-msvc -p operon-cli -p operond' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'scripts/verify-release-artifacts.sh "\$RELEASE_TAG" "\$GITHUB_REPOSITORY"' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'scripts/verify-release-install-usability.sh "\$RELEASE_TAG" "\$GITHUB_REPOSITORY"' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'scripts/verify-release-service-management-smoke.sh "\$RELEASE_TAG" "\$GITHUB_REPOSITORY"' .github/workflows/windows-runner-image-smoke.yml

require_pattern 'windows-2025' .github/workflows/ci.yml
require_pattern 'windows-2025' .github/workflows/live-mount-smoke.yml
require_pattern 'windows-2025' .github/workflows/release-draft.yml
require_pattern 'windows-2025' .github/workflows/verify-release-artifacts.yml
require_pattern 'windows-2025' .github/workflows/verify-release-install-usability.yml
reject_pattern 'windows-latest' .github/workflows/ci.yml
reject_pattern 'windows-latest' .github/workflows/live-mount-smoke.yml
reject_pattern 'windows-latest' .github/workflows/release-draft.yml
reject_pattern 'windows-latest' .github/workflows/verify-release-artifacts.yml
reject_pattern 'windows-latest' .github/workflows/verify-release-install-usability.yml

require_pattern 'windows-2025' scripts/verify-v0.11.3-platform-capability-matrix.sh
require_pattern 'windows-2025' scripts/verify-v0.12-release-distribution-readiness.sh
require_pattern 'windows-2025' scripts/verify-v0.12.4-release-artifact-verification.sh
require_pattern 'Windows Runner Image Smoke' docs/quality/release-ci-observability.md
require_pattern 'Windows Runner Image Smoke' DEVELOPMENT.md
require_pattern 'Windows Runner Image Smoke' AGENTS.md
require_pattern 'v0.18.9 Windows Runner Image Migration Smoke Validation' scripts/ci/run-validations.sh

bash -n scripts/verify-v0.18.9-windows-runner-image-migration-smoke.sh
scripts/verify-release-artifacts.sh --dry-run v0.16.7 >/dev/null
scripts/verify-release-install-usability.sh --dry-run v0.16.7 >/dev/null
scripts/verify-release-service-management-smoke.sh --dry-run v0.16.7 >/dev/null

echo "v0.18.9 Windows runner image migration smoke validation passed"
