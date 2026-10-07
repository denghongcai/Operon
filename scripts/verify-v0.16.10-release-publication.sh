#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
source scripts/lib/validation.sh

require_pattern 'Status: (In Progress|Completed)' docs/plan/v0.16.10-release-publication.md
require_pattern 'Phase 131: v0.16.10 Runtime Correctness and Performance Release' docs/plan/development-phases.md
for manifest in crates/*/Cargo.toml; do
  require_pattern 'version = "0.16.10"' "$manifest"
done
require_pattern '"version": "0.16.10"' packages/sdk-js/package.json
require_pattern 'PROTOCOL_VERSION: &str = "v0.16.10"' crates/operon-protocol/src/lib.rs
require_pattern 'assert_eq!\(PROTOCOL_VERSION, "v0.16.10"\)' crates/operon-protocol/src/conversions.rs
require_pattern 'stdout.contains\("0.16.10"\)' crates/operon-cli/tests/cli_static_integration.rs
for script in verify-release-install-usability verify-release-service-management-smoke verify-release-linux-install-containers; do
  require_pattern "$script.sh --dry-run v0.16.10" README.md
  require_pattern "$script.sh --dry-run v0.16.10" docs/quality/release-install-usability.md
  bash "scripts/$script.sh" --dry-run v0.16.10 denghongcai/Operon >/dev/null
done
require_pattern 'default: v0.16.10' .github/workflows/windows-runner-image-smoke.yml
require_pattern 'for example v0.16.10' .github/workflows/verify-release-install-usability.yml
require_pattern 'for example v0.16.10' .github/workflows/verify-readme-quickstart.yml
bash scripts/verify-release-artifacts.sh --dry-run v0.16.10 denghongcai/Operon >/dev/null
OPERON_VERSION=v0.16.10 bash scripts/verify-readme-quickstart-docker.sh --dry-run >/dev/null
bash scripts/release-gate-orchestrate.sh plan v0.16.10 HEAD denghongcai/Operon >/dev/null
echo "v0.16.10 release publication validation passed"
