#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.15.1-release-gate-hardening.md
require_file scripts/smoke-release-archive.sh
require_file scripts/verify-release-gates.sh

require_pattern 'Phase 96: v0.15.1 Release Gate Hardening' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.15.1-release-gate-hardening.md
require_pattern 'scripts/smoke-release-archive.sh' .github/workflows/release-draft.yml
require_pattern 'Smoke release archive' .github/workflows/release-draft.yml
require_pattern 'scripts/smoke-release-archive.sh "\$WORKDIR/assets/\$asset"' scripts/verify-release-artifacts.sh
require_pattern 'libfuse-t\.dylib' scripts/smoke-release-archive.sh
require_pattern '@executable_path' scripts/smoke-release-archive.sh
require_pattern 'DYLD_LIBRARY_PATH' scripts/smoke-release-archive.sh
require_pattern 'DYLD_FALLBACK_LIBRARY_PATH' scripts/smoke-release-archive.sh
require_pattern 'DYLD_FRAMEWORK_PATH' scripts/smoke-release-archive.sh
require_pattern 'Cross-Platform Live Mount Smoke' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'Cross-Platform Live Mount Smoke' scripts/verify-release-gates.sh
require_pattern 'v0.14 Live Mount Smoke' scripts/verify-release-gates.sh
require_pattern 'scripts/verify-release-gates.sh "\$GITHUB_REF_NAME" "\$GITHUB_SHA" "\$GITHUB_REPOSITORY"' .github/workflows/release-draft.yml
require_pattern 'scripts/verify-release-gates.sh "\$@"' scripts/verify-v0.14-release-gates.sh

bash -n scripts/smoke-release-archive.sh
bash -n scripts/verify-release-gates.sh
bash -n scripts/verify-v0.14-release-gates.sh
scripts/smoke-release-archive.sh --help >/dev/null
scripts/verify-release-gates.sh test-tag HEAD >/dev/null

echo "v0.15.1 release gate hardening validation passed"
