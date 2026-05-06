#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.17-release-ci-observability.md
require_file docs/quality/release-ci-observability.md
require_pattern 'Phase 103: v0.17 Release and CI Observability Cleanup' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.17-release-ci-observability.md
require_pattern 'OPERON_SKIP_SDK_TESTS=1' docs/quality/release-ci-observability.md
require_pattern 'Cross-Platform Live Mount Smoke' docs/quality/release-ci-observability.md
require_pattern 'Verify Release Artifacts' docs/quality/release-ci-observability.md
require_pattern 'Verify README Quickstart' docs/quality/release-ci-observability.md
require_pattern 'Cancel obsolete workflow runs' docs/quality/release-ci-observability.md
require_pattern 'v0.17 Release and CI Observability Cleanup Validation' scripts/ci/run-validations.sh

OPERON_SKIP_SDK_TESTS=1 scripts/verify-v0.16.2-sdk-maintainability-cleanup.sh

if command -v rustup >/dev/null 2>&1; then
  rustup target add x86_64-pc-windows-gnu >/dev/null
fi
cargo check -p operond --target x86_64-pc-windows-gnu --tests --locked

scripts/verify-release-artifacts.sh --dry-run v0.16.5 >/dev/null
OPERON_VERSION=v0.16.5 scripts/verify-readme-quickstart-docker.sh --dry-run >/dev/null

echo "v0.17 release and CI observability validation passed"
