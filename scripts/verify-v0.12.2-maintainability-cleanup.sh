#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.12.2-maintainability-cleanup.md
require_pattern 'Status: Completed' docs/plan/v0.12.2-maintainability-cleanup.md
require_pattern 'Phase 80: v0.12.2 Maintainability Cleanup' docs/plan/development-phases.md
require_pattern 'No v0.12.2 work remains' docs/plan/development-phases.md

require_file crates/operond/src/runtime.rs
require_pattern 'pub\(crate\) struct GrpcRuntime' crates/operond/src/runtime.rs
require_pattern 'impl OperonRuntime for GrpcRuntime' crates/operond/src/runtime.rs
reject_pattern 'impl OperonRuntime for GrpcRuntime' crates/operond/src/main.rs
require_pattern 'OperonRuntimeServer::new\(GrpcRuntime' crates/operond/src/main.rs

require_file crates/operon-cli/src/commands/exec_args.rs
require_file crates/operon-cli/src/commands/exec_session.rs
require_pattern 'pub\(crate\) mod exec_args' crates/operon-cli/src/commands/mod.rs
require_pattern 'pub\(crate\) mod exec_session' crates/operon-cli/src/commands/mod.rs
require_pattern 'run_request_from_cli' crates/operon-cli/src/commands/exec_args.rs
require_pattern 'command_from_cli_args' crates/operon-cli/src/commands/exec_args.rs
require_pattern 'pub\(crate\) async fn session' crates/operon-cli/src/commands/exec_session.rs
require_pattern 'RawModeGuard' crates/operon-cli/src/commands/exec_session.rs
require_pattern 'commands::exec_session::session' crates/operon-cli/src/cli_dispatch.rs
reject_pattern 'RawModeGuard' crates/operon-cli/src/commands/exec.rs
reject_pattern 'fn shell_escape_arg' crates/operon-cli/src/commands/exec.rs

cargo test -p operon-cli --locked exec_command_shell_escapes_multiple_cli_args
cargo test -p operon-cli --locked exec_session_terminal_dimensions
cargo test -p operond --locked fs_range_validation_rejects_overflow_and_large_chunks

echo "v0.12.2 maintainability cleanup validation passed"
