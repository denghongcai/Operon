#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.16.7-release-publication.md
require_pattern 'Status: Completed' docs/plan/v0.16.7-release-publication.md
require_pattern 'Phase 120: v0.16.7 Release Publication and Public Verification' docs/plan/development-phases.md
require_pattern 'v0.16.7 Release Publication and Public Verification Validation' scripts/ci/run-validations.sh

require_pattern 'Release prep commit `9af17148b521e77a7a3b571cf68c5b64925197b9` was pushed to' docs/plan/v0.16.7-release-publication.md
require_pattern 'Release prep commit `9af17148b521e77a7a3b571cf68c5b64925197b9` was pushed to' docs/plan/development-phases.md
require_pattern 'Published GitHub Release `v0.16.7`' docs/plan/v0.16.7-release-publication.md
require_pattern 'Published GitHub Release `v0.16.7`' docs/plan/development-phases.md
require_pattern 'No v0.16.7 release publication or public verification work remains' docs/plan/v0.16.7-release-publication.md
require_pattern 'No v0.16.7 release publication or public verification work remains' docs/plan/development-phases.md

bash -n scripts/verify-v0.16.7-release-publication.sh

echo "v0.16.7 release publication validation passed"
