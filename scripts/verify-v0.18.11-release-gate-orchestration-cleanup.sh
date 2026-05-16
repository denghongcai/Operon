#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.18.11-release-gate-orchestration-cleanup.md
require_pattern 'Status: Completed' docs/plan/v0.18.11-release-gate-orchestration-cleanup.md
require_pattern 'Phase 119: v0.18.11 Release Gate Orchestration Cleanup' docs/plan/development-phases.md
require_pattern 'No v0.18.11 release gate orchestration cleanup work remains' docs/plan/development-phases.md

require_file scripts/release-gate-orchestrate.sh
require_pattern 'plan <tag> <commit-sha>' scripts/release-gate-orchestrate.sh
require_pattern 'pretag <tag> <commit-sha>' scripts/release-gate-orchestrate.sh
require_pattern 'postrelease <tag> <commit-sha>' scripts/release-gate-orchestrate.sh
require_pattern 'Cross-Platform Live Mount Smoke' scripts/release-gate-orchestrate.sh
require_pattern 'Windows Runner Image Smoke' scripts/release-gate-orchestrate.sh
require_pattern 'Verify Release Artifacts' scripts/release-gate-orchestrate.sh
require_pattern 'Verify Release Install Usability' scripts/release-gate-orchestrate.sh
require_pattern 'Verify README Quickstart' scripts/release-gate-orchestrate.sh
require_pattern 'gh release edit "\$TAG" --repo "\$REPO" --draft=false' scripts/release-gate-orchestrate.sh

require_pattern 'release-gate-orchestrate.sh' DEVELOPMENT.md AGENTS.md docs/quality/release-ci-observability.md
require_pattern 'v0.18.11 Release Gate Orchestration Cleanup Validation' scripts/ci/run-validations.sh

bash -n scripts/release-gate-orchestrate.sh
bash -n scripts/verify-v0.18.11-release-gate-orchestration-cleanup.sh

plan_output="$(scripts/release-gate-orchestrate.sh plan v0.16.7 HEAD denghongcai/Operon)"
for expected in \
  'Release gate orchestration plan for denghongcai/Operon@v0.16.7 on commit HEAD' \
  'gh workflow run "Cross-Platform Live Mount Smoke"' \
  'gh workflow run "Windows Runner Image Smoke"' \
  'scripts/release-gate-orchestrate.sh pretag "v0.16.7" "HEAD" "denghongcai/Operon"' \
  'gh release edit "v0.16.7" --repo "denghongcai/Operon" --draft=false' \
  'gh workflow run "Verify Release Artifacts"' \
  'gh workflow run "Verify Release Install Usability"' \
  'gh workflow run "Verify README Quickstart"' \
  'scripts/release-gate-orchestrate.sh postrelease "v0.16.7" "HEAD" "denghongcai/Operon"'
do
  if ! grep -Fq "$expected" <<<"$plan_output"; then
    echo "release orchestration plan missing expected output: $expected" >&2
    echo "$plan_output" >&2
    exit 1
  fi
done

scripts/release-gate-orchestrate.sh pretag test-tag HEAD denghongcai/Operon >/dev/null
scripts/release-gate-orchestrate.sh postrelease test-tag HEAD denghongcai/Operon >/dev/null

echo "v0.18.11 release gate orchestration cleanup validation passed"
