#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.8.15-token-generation-panic-cleanup.md
require_file crates/operon-cli/src/private_files.rs
require_pattern 'const HEX: &\[u8; 16\]' crates/operon-cli/src/private_files.rs
reject_pattern 'writing to String should not fail' crates/operon-cli/src/private_files.rs
require_pattern 'v0.8.15 Token Generation Panic Cleanup' docs/plan/development-phases.md

cargo test -p operon-cli --locked

echo "v0.8.15 token generation panic cleanup validation passed"
