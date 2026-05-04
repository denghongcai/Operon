#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.12.5-cli-grpc-maintainability-cleanup.md
require_pattern 'Status: Completed' docs/plan/v0.12.5-cli-grpc-maintainability-cleanup.md
require_pattern 'Phase 83: v0.12.5 CLI gRPC Maintainability Cleanup' docs/plan/development-phases.md
require_pattern 'No v0.12.5 work remains' docs/plan/development-phases.md

require_file crates/operon-cli/src/grpc.rs
require_file crates/operon-cli/src/grpc_fs.rs
require_file crates/operon-cli/src/grpc_exec_api.rs
require_file crates/operon-cli/src/grpc_service_api.rs
require_file crates/operon-cli/src/grpc_audit.rs

require_pattern 'mod grpc_fs' crates/operon-cli/src/main.rs
require_pattern 'mod grpc_exec_api' crates/operon-cli/src/main.rs
require_pattern 'mod grpc_service_api' crates/operon-cli/src/main.rs
require_pattern 'mod grpc_audit' crates/operon-cli/src/main.rs
require_pattern 'pub use crate::grpc_fs' crates/operon-cli/src/grpc.rs
require_pattern 'pub use crate::grpc_exec_api' crates/operon-cli/src/grpc.rs
require_pattern 'pub use crate::grpc_service_api' crates/operon-cli/src/grpc.rs
require_pattern 'pub use crate::grpc_audit' crates/operon-cli/src/grpc.rs

require_pattern 'pub async fn fs_stat' crates/operon-cli/src/grpc_fs.rs
require_pattern 'pub async fn write_file_bytes' crates/operon-cli/src/grpc_fs.rs
require_pattern 'chunks_write_requests_use_target_then_data_chunks' crates/operon-cli/src/grpc_fs.rs
require_pattern 'pub async fn run_exec' crates/operon-cli/src/grpc_exec_api.rs
require_pattern 'pub async fn stream_exec_logs' crates/operon-cli/src/grpc_exec_api.rs
require_pattern 'pub async fn list_services' crates/operon-cli/src/grpc_service_api.rs
require_pattern 'pub async fn list_audit' crates/operon-cli/src/grpc_audit.rs

reject_pattern 'pub async fn fs_stat' crates/operon-cli/src/grpc.rs
reject_pattern 'pub async fn run_exec' crates/operon-cli/src/grpc.rs
reject_pattern 'pub async fn list_services' crates/operon-cli/src/grpc.rs
reject_pattern 'pub async fn list_audit' crates/operon-cli/src/grpc.rs

grpc_lines="$(wc -l < crates/operon-cli/src/grpc.rs)"
if (( grpc_lines > 260 )); then
  echo "crates/operon-cli/src/grpc.rs is too large after cleanup: ${grpc_lines} lines" >&2
  exit 1
fi

require_pattern 'v0.12.5 CLI gRPC Maintainability Cleanup Validation' scripts/ci/run-validations.sh

cargo test -p operon-cli --locked chunks_write_requests_use_target_then_data_chunks
cargo test -p operon-cli --locked with_auth_includes_execution_context_metadata
cargo check -p operon-cli --locked

echo "v0.12.5 CLI gRPC maintainability cleanup validation passed"
