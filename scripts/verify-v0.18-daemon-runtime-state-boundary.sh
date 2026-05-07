#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.18-daemon-runtime-state-boundary.md
require_pattern 'Phase 109: v0.18 Daemon Runtime / State Boundary Cleanup' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.18-daemon-runtime-state-boundary.md
require_pattern 'No v0.18 daemon runtime/state boundary cleanup work remains' docs/plan/development-phases.md

require_file crates/operond/src/daemon_state.rs
require_pattern 'mod daemon_state;' crates/operond/src/main.rs
require_pattern 'struct LoadedDaemonRuntime' crates/operond/src/daemon_state.rs
require_pattern 'fn load_daemon_runtime' crates/operond/src/daemon_state.rs
require_pattern 'fn load_config' crates/operond/src/daemon_state.rs
require_pattern 'OperonConfig::from_str_with_warnings' crates/operond/src/daemon_state.rs
require_pattern 'fn load_secrets' crates/operond/src/daemon_state.rs
require_pattern 'bounded_audit_events' crates/operond/src/daemon_state.rs
require_pattern 'exec_log_buffers_from_persisted_logs' crates/operond/src/daemon_state.rs
require_pattern 'capabilities_from_policy' crates/operond/src/daemon_state.rs
require_pattern 'test_state_derives_capabilities_from_supplied_policy' crates/operond/src/daemon_state.rs
require_pattern 'daemon_state::load_daemon_runtime' crates/operond/src/main.rs
reject_pattern 'OperonConfig::load' crates/operond/src/main.rs
reject_pattern 'fn load_secrets' crates/operond/src/main.rs
reject_pattern 'fn hostname' crates/operond/src/main.rs

cargo check -p operond --locked
cargo test -p operond --locked daemon_state
cargo test -p operond --locked persisted_exec_logs_seed_bounded_log_buffers

echo "v0.18 daemon runtime/state boundary validation passed"
