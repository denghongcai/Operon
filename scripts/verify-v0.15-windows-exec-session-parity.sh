#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.15-windows-exec-session-parity.md
require_pattern 'Phase 95: v0.15 Windows Exec Session Parity' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.15-windows-exec-session-parity.md

reject_pattern 'WINDOWS_EXEC_SESSION_UNSUPPORTED_REASON' crates/operond/src/exec_session.rs
reject_pattern 'Status::unimplemented' crates/operond/src/exec_session.rs
reject_pattern 'windows_exec_session_is_explicitly_unsupported' crates/operond/src/exec_session.rs .github/workflows/ci.yml
reject_pattern 'windows-exec-session-unsupported' crates/operon-cli/src/commands/doctor.rs README.md PROTOCOL.md docs/architecture/runtime-api.md
reject_pattern 'Windows interactive exec sessions are explicitly unsupported' README.md PROTOCOL.md docs/architecture/runtime-api.md

require_pattern 'fn exec_session_platform_is_supported' crates/operond/src/exec_session.rs
require_pattern 'cargo test -p operond --locked exec_session_platform_is_supported' .github/workflows/ci.yml
require_pattern 'cargo test -p operond --locked exec_session_portable_pty_smoke_outputs_and_exits' .github/workflows/ci.yml
require_pattern 'windows-portable-pty-smoke-validated' crates/operon-cli/src/commands/doctor.rs
require_pattern 'Windows interactive exec sessions use `portable-pty`' README.md PROTOCOL.md docs/architecture/runtime-api.md
require_pattern 'PROTOCOL_VERSION: &str = "v0.16.5"' crates/operon-protocol/src/lib.rs
require_pattern '"version": "0.16.5"' packages/sdk-js/package.json

cargo test -p operond --locked exec_session_platform_is_supported
cargo test -p operond --locked exec_session_portable_pty_smoke_outputs_and_exits
cargo test -p operon-cli --locked platform_report_contains_operator_caveats

echo "v0.15 Windows exec session parity validation passed"
