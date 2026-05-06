#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.16.3-daemon-mount-maintainability-cleanup.md
require_pattern 'Phase 100: v0.16.3 Daemon and Mount Maintainability Cleanup' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.16.3-daemon-mount-maintainability-cleanup.md
require_file crates/operond/src/exec_command.rs
require_file crates/operon-cli/src/commands/mount_runtime.rs
require_pattern 'mod exec_command;' crates/operond/src/main.rs
require_pattern 'exec_command::build_exec_command' crates/operond/src/exec_runtime.rs
require_pattern 'pub\(crate\) fn build_exec_command' crates/operond/src/exec_command.rs
require_pattern 'pub\(crate\) fn report' crates/operon-cli/src/commands/mount_runtime.rs
require_pattern 'commands::mount_runtime' crates/operon-cli/src/commands/doctor.rs
require_pattern 'commands::mount_runtime' crates/operon-cli/src/commands/mount.rs

cargo test -p operond --locked exec_shell_invocation_matches_platform
cargo test -p operon-cli --locked mount_runtime

echo "v0.16.3 daemon/mount maintainability cleanup validation passed"
