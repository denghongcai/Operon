#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.13.1-windows-pty-validation.md
require_pattern 'Status: Completed' docs/plan/v0.13.1-windows-pty-validation.md
require_pattern 'Phase 85: v0.13.1 Windows PTY Validation' docs/plan/development-phases.md
require_pattern 'No v0.13.1 Windows PTY validation work remains' docs/plan/development-phases.md

require_pattern 'exec_session_platform_is_supported' crates/operond/src/exec_session.rs
require_pattern 'exec_session_portable_pty_smoke_outputs_and_exits' crates/operond/src/exec_session.rs
require_pattern 'windows-portable-pty-smoke-validated' crates/operon-cli/src/commands/doctor.rs
reject_pattern 'portable-pty-validation-deferred' crates/operon-cli/src/commands/doctor.rs README.md PROTOCOL.md docs/architecture/runtime-api.md
reject_pattern 'windows-exec-session-unsupported' crates/operon-cli/src/commands/doctor.rs README.md PROTOCOL.md docs/architecture/runtime-api.md

require_pattern 'cargo test -p operond --locked exec_session_platform_is_supported' .github/workflows/ci.yml
require_pattern 'cargo test -p operond --locked exec_session_portable_pty_smoke_outputs_and_exits' .github/workflows/ci.yml

require_pattern 'Windows interactive exec sessions use `portable-pty`' README.md
require_pattern 'Windows interactive exec sessions use `portable-pty`' PROTOCOL.md
require_pattern 'Windows interactive exec sessions use `portable-pty`' docs/architecture/runtime-api.md
require_pattern 'supported through `portable-pty` on Unix-like platforms and Windows' docs/architecture/technology-and-protocol-decisions.md

cargo test -p operond --locked exec_session_platform_is_supported
cargo test -p operond --locked exec_session_portable_pty_smoke_outputs_and_exits
cargo test -p operon-cli --locked platform_report_contains_operator_caveats

echo "v0.13.1 Windows PTY validation passed"
