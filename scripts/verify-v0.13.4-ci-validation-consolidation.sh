#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.13.4-ci-validation-consolidation.md
require_pattern 'Status: Completed' docs/plan/v0.13.4-ci-validation-consolidation.md
require_pattern 'Phase 88: v0.13.4 CI Validation Consolidation' docs/plan/development-phases.md
require_pattern 'No v0.13.4 CI validation consolidation work remains' docs/plan/development-phases.md

require_file scripts/ci/run-validations.sh
require_pattern 'failures=\(\)' scripts/ci/run-validations.sh
require_pattern '::group::\$name' scripts/ci/run-validations.sh
require_pattern 'Validation failures:' scripts/ci/run-validations.sh
require_pattern 'scripts/verify-v0.13.4-ci-validation-consolidation.sh' scripts/ci/run-validations.sh

require_pattern 'name: Validation' .github/workflows/ci.yml
require_pattern 'scripts/ci/run-validations.sh' .github/workflows/ci.yml
reject_pattern 'name: \$\{\{ matrix\.name \}\}' .github/workflows/ci.yml
reject_pattern 'script: scripts/verify-v0\.' .github/workflows/ci.yml

require_pattern 'scripts/ci/run-validations.sh' DEVELOPMENT.md
require_pattern 'version validation scripts should be added to `scripts/ci/run-validations.sh`' DEVELOPMENT.md
require_pattern 'must be wired through' AGENTS.md
require_pattern 'scripts/ci/run-validations.sh' AGENTS.md

bash -n scripts/ci/run-validations.sh

echo "v0.13.4 CI validation consolidation validation passed"
