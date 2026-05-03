#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file crates/operond/src/exec_service.rs
require_file crates/operond/src/exec_session.rs
require_file crates/operon-cli/src/grpc_exec.rs

require_pattern 'mod exec_service;' crates/operond/src/main.rs
require_pattern 'mod exec_session;' crates/operond/src/main.rs
require_pattern 'mod grpc_exec;' crates/operon-cli/src/main.rs
require_pattern 'open_exec_session' crates/operond/src/exec_service.rs
require_pattern 'open_exec_session' crates/operond/src/exec_session.rs
require_pattern 'open_exec_session' crates/operon-cli/src/grpc_exec.rs

reject_pattern 'stdin stream target metadata was sent more than once' crates/operond/src/main.rs
reject_pattern 'stream_exec_logs_to_writer' crates/operon-cli/src/grpc.rs

echo "v0.10.4 maintainability cleanup validation passed"
