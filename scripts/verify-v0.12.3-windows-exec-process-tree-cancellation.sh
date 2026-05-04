#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.12.3-windows-exec-process-tree-cancellation.md
require_pattern 'Status: Completed' docs/plan/v0.12.3-windows-exec-process-tree-cancellation.md
require_pattern 'Phase 81: v0.12.3 Windows Exec Process-Tree Cancellation' docs/plan/development-phases.md
require_pattern 'No v0.12.3 work remains' docs/plan/development-phases.md

require_pattern 'windows-sys = "0\.61\.2"' Cargo.toml
require_pattern 'Win32_System_JobObjects' crates/operond/Cargo.toml
require_pattern 'struct ExecChildGroup' crates/operond/src/exec_runtime.rs
require_pattern 'struct WindowsJobObject' crates/operond/src/exec_runtime.rs
require_pattern 'AssignProcessToJobObject' crates/operond/src/exec_runtime.rs
require_pattern 'TerminateJobObject' crates/operond/src/exec_runtime.rs
require_pattern 'job-object-process-tree' crates/operond/src/exec_runtime.rs
require_pattern 'windows_job_object_cancellation_terminates_descendant_process' crates/operond/src/exec_runtime.rs
require_pattern 'job-object-process-tree-termination' crates/operon-cli/src/commands/doctor.rs

require_pattern 'windows_job_object_cancellation_terminates_descendant_process' .github/workflows/ci.yml
require_pattern "runner.os == 'Windows'" .github/workflows/ci.yml
require_pattern 'v0.12.3 Windows Exec Process Tree Cancellation Validation' .github/workflows/ci.yml

require_pattern 'Windows non-interactive exec cancellation uses Job Object process-tree' README.md
require_pattern 'Job Object process-tree termination' PROTOCOL.md
require_pattern 'direct-child' PROTOCOL.md
require_pattern 'Job Object process-tree' docs/architecture/runtime-api.md

cargo test -p operond --locked exec_cancellation_guarantee_matches_platform
if ! rustup target list --installed | rg '^x86_64-pc-windows-gnu$' >/dev/null; then
  rustup target add x86_64-pc-windows-gnu
fi
cargo check -p operond --locked --target x86_64-pc-windows-gnu

echo "v0.12.3 Windows exec process-tree cancellation validation passed"
