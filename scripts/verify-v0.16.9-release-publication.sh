#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.16.9-release-publication.md
require_pattern 'Status: (In Progress|Completed)' docs/plan/v0.16.9-release-publication.md
require_pattern 'Phase 124: v0.16.9 Architecture Boundary Release Publication' docs/plan/development-phases.md
require_pattern 'v0.16.9 Architecture Boundary Release Publication Validation' scripts/ci/run-validations.sh

# Historical release evidence stays at v0.16.9; current version alignment is
# validated by the active release phase, not frozen to this historical tag.
require_pattern 'https://github.com/denghongcai/Operon/releases/tag/v0.16.9' docs/plan/v0.16.9-release-publication.md

bash -n scripts/verify-v0.16.9-release-publication.sh
scripts/verify-release-artifacts.sh --dry-run v0.16.9 denghongcai/Operon >/dev/null
scripts/verify-release-install-usability.sh --dry-run v0.16.9 denghongcai/Operon >/dev/null
scripts/verify-release-service-management-smoke.sh --dry-run v0.16.9 denghongcai/Operon >/dev/null
scripts/verify-release-linux-install-containers.sh --dry-run v0.16.9 denghongcai/Operon >/dev/null
OPERON_VERSION=v0.16.9 scripts/verify-readme-quickstart-docker.sh --dry-run >/dev/null
scripts/release-gate-orchestrate.sh plan v0.16.9 HEAD denghongcai/Operon >/dev/null

echo "v0.16.9 release publication validation passed"
