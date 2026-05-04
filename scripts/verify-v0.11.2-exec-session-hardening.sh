#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.11.2-exec-session-hardening.md
require_pattern 'Status: Completed' docs/plan/v0.11.2-exec-session-hardening.md
require_pattern 'v0.11.2 Exec Session Hardening' docs/plan/development-phases.md
require_pattern 'No v0.11.2 work remains' docs/plan/development-phases.md

require_pattern 'TerminalDimensions' crates/operon-cli/src/commands/exec_session.rs
require_pattern 'local_terminal_dimensions_or_default' crates/operon-cli/src/commands/exec_session.rs
require_pattern 'spawn_resize_forwarder' crates/operon-cli/src/grpc_exec.rs
require_pattern 'ExecSessionResize' crates/operon-cli/src/grpc_exec.rs
require_pattern 'SessionStreamGuard' crates/operond/src/exec_session.rs
require_pattern 'SessionControl::Terminate' crates/operond/src/exec_session.rs
require_pattern 'portable-pty' docs/plan/v0.11.2-exec-session-hardening.md
require_pattern 'macOS and Windows PTY validation remains future platform/distribution work' docs/plan/v0.11.2-exec-session-hardening.md

cargo test -p operon-cli --locked exec_session_terminal_dimensions
cargo test -p operond --locked exec_session_stream_guard
scripts/verify-v0.11-exec-session.sh

echo "v0.11.2 exec session hardening validation passed"
