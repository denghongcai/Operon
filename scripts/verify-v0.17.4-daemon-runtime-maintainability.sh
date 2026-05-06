#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.17.4-daemon-runtime-maintainability.md
require_pattern 'Phase 107: v0.17.4 Daemon Runtime Maintainability Cleanup' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.17.4-daemon-runtime-maintainability.md

require_file crates/operond/src/exec_process.rs
require_pattern 'mod exec_process;' crates/operond/src/main.rs
require_pattern 'pub\(crate\) struct ExecChildGroup' crates/operond/src/exec_process.rs
require_pattern 'pub\(crate\) async fn terminate_child' crates/operond/src/exec_process.rs
require_pattern 'pub\(crate\) async fn pump_exec_stdin' crates/operond/src/exec_process.rs
require_pattern 'pub\(crate\) async fn capture_exec_stream' crates/operond/src/exec_process.rs
require_pattern 'exec_process::\{' crates/operond/src/exec_runtime.rs

if rg -q 'pub\(crate\) struct ExecChildGroup|fn terminate_child_process_group|async fn pump_exec_stdin|async fn capture_exec_stream' crates/operond/src/exec_runtime.rs; then
  echo "exec_runtime.rs should not own child process lifecycle or stdio helper definitions after v0.17.4" >&2
  exit 1
fi

cargo check -p operond --locked
cargo test -p operond --locked exec_cancellation_guarantee_matches_platform
cargo test -p operond --locked persisted_exec_logs_seed_bounded_log_buffers
if command -v rustup >/dev/null 2>&1; then
  rustup target add x86_64-pc-windows-gnu >/dev/null
fi
cargo check -p operond --target x86_64-pc-windows-gnu --tests --locked

echo "v0.17.4 daemon runtime maintainability validation passed"
