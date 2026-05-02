#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.8.11-cli-datagram-lock-hardening.md
require_file crates/operon-cli/src/grpc.rs
require_pattern 'anyhow::Result<String>' crates/operon-cli/src/grpc.rs
require_pattern 'peer_addr_for_id' crates/operon-cli/src/grpc.rs
require_pattern 'remove_datagram_peer' crates/operon-cli/src/grpc.rs
reject_pattern 'expect\("datagram peer state poisoned"\)' crates/operon-cli/src/grpc.rs
require_pattern 'v0.8.11 CLI Datagram Lock Hardening' docs/plan/development-phases.md

cargo test -p operon-cli --locked

echo "v0.8.11 CLI datagram lock hardening validation passed"
