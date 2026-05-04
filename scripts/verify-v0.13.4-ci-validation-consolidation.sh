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
require_pattern 'requested_group=' scripts/ci/run-validations.sh
require_pattern '::group::\[\$group\] \$name' scripts/ci/run-validations.sh
require_pattern 'Validation failures:' scripts/ci/run-validations.sh
require_pattern 'core\|v0.13.4 CI Validation Consolidation\|scripts/verify-v0.13.4-ci-validation-consolidation.sh' scripts/ci/run-validations.sh
require_pattern 'runtime\|v0.11 Exec Session Validation\|scripts/verify-v0.11-exec-session.sh' scripts/ci/run-validations.sh
require_pattern 'sdk\|v0.10.1 Filesystem Consistency and Workspace Hardening Validation\|scripts/verify-v0.10.1-fs-consistency-workspace-hardening.sh' scripts/ci/run-validations.sh
require_pattern 'linux-system\|v0.6 Linux Mount Validation\|scripts/verify-v0.6-linux-mount.sh' scripts/ci/run-validations.sh
require_pattern 'scripts/verify-v0.13.4-ci-validation-consolidation.sh' scripts/ci/run-validations.sh

require_pattern 'name: Validation \(\$\{\{ matrix\.group \}\}\)' .github/workflows/ci.yml
require_pattern 'OPERON_SKIP_SDK_TESTS: "1"' .github/workflows/ci.yml
require_pattern 'scripts/ci/run-validations.sh "\$\{\{ matrix\.group \}\}"' .github/workflows/ci.yml
reject_pattern 'name: \$\{\{ matrix\.name \}\}' .github/workflows/ci.yml
reject_pattern 'script: scripts/verify-v0\.' .github/workflows/ci.yml

require_pattern 'scripts/ci/run-validations.sh' DEVELOPMENT.md
require_pattern 'assigned to the narrowest existing group' DEVELOPMENT.md
require_pattern 'OPERON_SKIP_SDK_TESTS=1' DEVELOPMENT.md
require_pattern 'must be wired through' AGENTS.md
require_pattern 'scripts/ci/run-validations.sh' AGENTS.md
require_pattern 'OPERON_SKIP_SDK_TESTS=1' AGENTS.md

bash -n scripts/ci/run-validations.sh

echo "v0.13.4 CI validation consolidation validation passed"
