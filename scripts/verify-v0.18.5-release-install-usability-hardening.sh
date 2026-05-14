#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.18.5-release-install-usability-hardening.md
require_file scripts/verify-release-install-usability.sh
require_file scripts/verify-release-linux-install-containers.sh
require_file .github/workflows/verify-release-install-usability.yml
require_file docs/quality/release-install-usability.md

require_pattern 'Status: Completed' docs/plan/v0.18.5-release-install-usability-hardening.md
require_pattern 'Phase 115: v0.18.5 Release / Install Usability Hardening' docs/plan/development-phases.md
require_pattern 'scripts/verify-release-install-usability.sh' docs/plan/v0.18.5-release-install-usability-hardening.md
require_pattern 'scripts/verify-release-linux-install-containers.sh' docs/plan/v0.18.5-release-install-usability-hardening.md
require_pattern 'Verify Release Install Usability' docs/quality/release-install-usability.md
require_pattern 'Verify Release Install Usability' .github/workflows/verify-release-install-usability.yml
require_pattern 'ubuntu:20.04' scripts/verify-release-linux-install-containers.sh
require_pattern 'debian:12' scripts/verify-release-linux-install-containers.sh
require_pattern 'PATH points at isolated install prefix' scripts/verify-release-install-usability.sh
require_pattern 'operon doctor --mount-runtime' scripts/verify-release-install-usability.sh
require_pattern 'verify-release-install-usability.sh --dry-run v0.16.6' README.md
require_pattern 'v0.18.5 Release / Install Usability Hardening Validation' scripts/ci/run-validations.sh

scripts/verify-release-install-usability.sh --dry-run v0.16.6 denghongcai/Operon >/dev/null
scripts/verify-release-linux-install-containers.sh --dry-run v0.16.6 denghongcai/Operon >/dev/null

bash -n scripts/verify-release-install-usability.sh
bash -n scripts/verify-release-linux-install-containers.sh
bash -n scripts/verify-v0.18.5-release-install-usability-hardening.sh

echo "v0.18.5 release/install usability hardening validation passed"
