#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
source scripts/lib/validation.sh

require_pattern 'Status: (In Progress|Completed)' docs/plan/v0.16.10-release-publication.md
require_pattern 'Phase 131: v0.16.10 Runtime Correctness and Performance Release' docs/plan/development-phases.md
# Historical evidence remains pinned; current alignment belongs to the active release gate.
require_pattern 'https://github.com/denghongcai/Operon/releases/tag/v0.16.10' docs/plan/v0.16.10-release-publication.md
bash scripts/verify-release-artifacts.sh --dry-run v0.16.10 denghongcai/Operon >/dev/null
bash scripts/release-gate-orchestrate.sh plan v0.16.10 HEAD denghongcai/Operon >/dev/null
echo "v0.16.10 historical release publication validation passed"
