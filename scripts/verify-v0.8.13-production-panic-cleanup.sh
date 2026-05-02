#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.8.13-production-panic-cleanup.md
require_file crates/operond/src/job_runtime.rs
require_file crates/operon-mount/src/remote_client.rs
require_pattern 'job log buffer unexpectedly empty after append' crates/operond/src/job_runtime.rs
reject_pattern 'expect\("just pushed job log"\)' crates/operond/src/job_runtime.rs
require_pattern 'remote fs runtime is unavailable' crates/operon-mount/src/remote_client.rs
reject_pattern 'expect\("remote fs runtime is only cleared during drop"\)' crates/operon-mount/src/remote_client.rs
require_pattern 'v0.8.13 Production Panic Cleanup' docs/plan/development-phases.md

cargo test -p operond --locked
cargo test -p operon-mount --locked

echo "v0.8.13 production panic cleanup validation passed"
