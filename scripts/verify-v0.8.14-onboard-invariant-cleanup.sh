#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.8.14-onboard-invariant-cleanup.md
require_file crates/operon-cli/src/onboard.rs
require_pattern 'daemon onboarding token is unavailable' crates/operon-cli/src/onboard.rs
reject_pattern 'expect\("daemon onboarding should have a token"\)' crates/operon-cli/src/onboard.rs
require_pattern 'v0.8.14 Onboard Invariant Cleanup' docs/plan/development-phases.md

cargo test -p operon-cli --locked

echo "v0.8.14 onboard invariant cleanup validation passed"
