#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
source scripts/lib/validation.sh
require_pattern 'Status: (In Progress|Completed)' docs/plan/v0.16.11-release-publication.md
require_pattern 'Phase 136: v0.16.11 Acceptance and Public Release' docs/plan/development-phases.md
# Preserve historical evidence; current alignment belongs to the active release.
require_pattern 'https://github.com/denghongcai/Operon/releases/tag/v0.16.11' docs/plan/v0.16.11-release-publication.md
for script in verify-release-install-usability verify-release-service-management-smoke verify-release-linux-install-containers; do
  bash "scripts/$script.sh" --dry-run v0.16.11 denghongcai/Operon >/dev/null
done
bash scripts/verify-release-artifacts.sh --dry-run v0.16.11 denghongcai/Operon >/dev/null
OPERON_VERSION=v0.16.11 bash scripts/verify-readme-quickstart-docker.sh --dry-run >/dev/null
bash scripts/release-gate-orchestrate.sh plan v0.16.11 HEAD denghongcai/Operon >/dev/null
echo "v0.16.11 historical release publication validation passed"
