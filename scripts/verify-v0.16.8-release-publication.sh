#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.16.8-release-publication.md
require_pattern 'Status: Completed' docs/plan/v0.16.8-release-publication.md
require_pattern 'Phase 121: v0.16.8 Security Hardening Release Publication' docs/plan/development-phases.md
require_pattern 'v0.16.8 Security Hardening Release Publication Validation' scripts/ci/run-validations.sh
require_pattern 'internal.*maintenance.*batch identifiers' docs/plan/development-phases.md
require_pattern 'cleanup batch identifiers' docs/plan/v0.16.8-release-publication.md

bash -n scripts/verify-v0.16.8-release-publication.sh
require_pattern 'https://github.com/denghongcai/Operon/releases/tag/v0.16.8' docs/plan/v0.16.8-release-publication.md

echo "v0.16.8 release publication validation passed"
