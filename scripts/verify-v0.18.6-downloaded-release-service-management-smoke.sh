#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.18.6-downloaded-release-service-management-smoke.md
require_pattern 'Status: Completed' docs/plan/v0.18.6-downloaded-release-service-management-smoke.md
require_pattern 'Phase 116: v0.18.6 Downloaded Release Service-Management Smoke' docs/plan/development-phases.md
require_pattern 'No v0.18.6 downloaded release service-management smoke work remains' docs/plan/development-phases.md

require_file scripts/lib/release-install.sh
require_file scripts/verify-release-service-management-smoke.sh
require_pattern 'release_install_setup' scripts/lib/release-install.sh scripts/verify-release-install-usability.sh scripts/verify-release-service-management-smoke.sh
require_pattern 'systemctl' scripts/verify-release-service-management-smoke.sh
require_pattern 'launchctl' scripts/verify-release-service-management-smoke.sh
require_pattern 'sc.exe' scripts/verify-release-service-management-smoke.sh
require_pattern 'operond service install' scripts/verify-release-service-management-smoke.sh
require_pattern 'Verify release service management smoke' .github/workflows/verify-release-install-usability.yml
require_pattern 'scripts/verify-release-service-management-smoke.sh' docs/quality/release-install-usability.md README.md DEVELOPMENT.md AGENTS.md scripts/ci/run-validations.sh
require_pattern 'v0.18.6 Downloaded Release Service-Management Smoke Validation' scripts/ci/run-validations.sh

bash -n scripts/lib/release-install.sh
bash -n scripts/verify-release-install-usability.sh
bash -n scripts/verify-release-service-management-smoke.sh
bash -n scripts/verify-v0.18.6-downloaded-release-service-management-smoke.sh

dry_run="$(scripts/verify-release-service-management-smoke.sh --dry-run v0.16.7 denghongcai/Operon)"
for expected in \
  'tag=v0.16.7' \
  'asset=operon-v0.16.7-linux-x86_64.tar.gz' \
  'operond service install --config' \
  'fake systemctl/launchctl/sc.exe supervisor smoke'
do
  if ! grep -Fq "$expected" <<<"$dry_run"; then
    echo "service-management dry run missing expected output: $expected" >&2
    echo "$dry_run" >&2
    exit 1
  fi
done

echo "v0.18.6 downloaded release service-management smoke validation passed"
