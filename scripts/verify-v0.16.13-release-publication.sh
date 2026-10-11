#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
source scripts/lib/validation.sh
require_pattern 'Status: (In Progress|Completed)' docs/plan/v0.16.13-release-publication.md
require_pattern 'Phase 146: v0.16.13 Parallel Read Public Release' docs/plan/development-phases.md
# Historical evidence stays pinned; current alignment belongs to v0.16.14.
require_pattern 'https://github.com/denghongcai/Operon/releases/tag/v0.16.13' docs/plan/v0.16.13-release-publication.md
for script in verify-release-install-usability verify-release-service-management-smoke verify-release-linux-install-containers; do
  bash "scripts/$script.sh" --dry-run v0.16.13 denghongcai/Operon >/dev/null
done
bash scripts/verify-release-artifacts.sh --dry-run v0.16.13 denghongcai/Operon >/dev/null
OPERON_VERSION=v0.16.13 bash scripts/verify-readme-quickstart-docker.sh --dry-run >/dev/null
bash scripts/release-gate-orchestrate.sh plan v0.16.13 HEAD denghongcai/Operon >/dev/null
bash scripts/verify-musl-release-integration.sh
echo 'v0.16.13 historical release publication validation passed'
