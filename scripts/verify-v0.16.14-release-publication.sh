#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
source scripts/lib/validation.sh
require_pattern 'Status: (In Progress|Completed)' docs/plan/v0.16.14-release-publication.md
require_pattern 'Phase 152: Verified Async Read Public Release' docs/plan/development-phases.md
# Historical release evidence stays immutable; current source alignment is
# validated by the newer v0.16.15 publication gate.
for script in verify-release-install-usability verify-release-service-management-smoke verify-release-linux-install-containers; do
  bash "scripts/$script.sh" --dry-run v0.16.14 denghongcai/Operon >/dev/null
done
bash scripts/verify-release-artifacts.sh --dry-run v0.16.14 denghongcai/Operon >/dev/null
OPERON_VERSION=v0.16.14 bash scripts/verify-readme-quickstart-docker.sh --dry-run >/dev/null
bash scripts/release-gate-orchestrate.sh plan v0.16.14 HEAD denghongcai/Operon >/dev/null
bash scripts/verify-musl-release-integration.sh
echo 'v0.16.14 release preparation validation passed'
