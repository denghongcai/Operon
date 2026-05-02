#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.8.12-daemon-datagram-invariant-cleanup.md
require_file crates/operond/src/service_forward.rs
require_pattern 'service datagram session is missing' crates/operond/src/service_forward.rs
reject_pattern 'expect\("session should exist after creation"\)' crates/operond/src/service_forward.rs
require_pattern 'v0.8.12 Daemon Datagram Invariant Cleanup' docs/plan/development-phases.md

cargo test -p operond --locked

echo "v0.8.12 daemon datagram invariant cleanup validation passed"
