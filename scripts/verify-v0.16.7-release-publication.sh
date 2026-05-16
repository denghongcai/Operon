#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.16.7-release-publication.md
require_pattern 'Status: (In Progress|Completed)' docs/plan/v0.16.7-release-publication.md
require_pattern 'Phase 120: v0.16.7 Release Publication and Public Verification' docs/plan/development-phases.md
require_pattern 'v0.16.7 Release Publication and Public Verification Validation' scripts/ci/run-validations.sh

for crate_manifest in crates/*/Cargo.toml; do
  require_pattern 'version = "0.16.7"' "$crate_manifest"
done
require_pattern '"version": "0.16.7"' packages/sdk-js/package.json
require_pattern 'PROTOCOL_VERSION: &str = "v0.16.7"' crates/operon-protocol/src/lib.rs
require_pattern 'assert_eq!\(PROTOCOL_VERSION, "v0.16.7"\)' crates/operon-protocol/src/lib.rs
require_pattern 'stdout.contains\("0.16.7"\)' crates/operon-cli/tests/cli_static_integration.rs

require_pattern 'verify-release-install-usability.sh --dry-run v0.16.7' README.md docs/quality/release-install-usability.md
require_pattern 'verify-release-service-management-smoke.sh --dry-run v0.16.7' README.md docs/quality/release-install-usability.md
require_pattern 'verify-release-linux-install-containers.sh --dry-run v0.16.7' README.md docs/quality/release-install-usability.md
require_pattern 'default: v0.16.7' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'for example v0.16.7' .github/workflows/verify-release-install-usability.yml
require_pattern 'for example v0.16.7' .github/workflows/verify-readme-quickstart.yml

bash -n scripts/verify-v0.16.7-release-publication.sh
scripts/verify-release-artifacts.sh --dry-run v0.16.7 >/dev/null
scripts/verify-release-install-usability.sh --dry-run v0.16.7 denghongcai/Operon >/dev/null
scripts/verify-release-service-management-smoke.sh --dry-run v0.16.7 denghongcai/Operon >/dev/null
scripts/verify-release-linux-install-containers.sh --dry-run v0.16.7 denghongcai/Operon >/dev/null
OPERON_VERSION=v0.16.7 scripts/verify-readme-quickstart-docker.sh --dry-run >/dev/null
scripts/release-gate-orchestrate.sh plan v0.16.7 HEAD denghongcai/Operon >/dev/null

echo "v0.16.7 release publication validation passed"
