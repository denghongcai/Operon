#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.9.4-runtime-hardening-consolidation.md
require_pattern 'Status: Completed' docs/plan/v0.9.4-runtime-hardening-consolidation.md
require_pattern 'v0.9.4 Runtime Hardening Consolidation' docs/plan/development-phases.md
require_pattern 'No v0.9.4 work remains' docs/plan/development-phases.md

require_pattern 'PROTOCOL_VERSION: &str = "v0.9.7"' crates/operon-protocol/src/lib.rs
require_pattern 'repeated string argv = 6' proto/operon/runtime.proto
require_pattern 'argv: value.argv' crates/operon-protocol/src/lib.rs
require_pattern 'Execute CLI words as argv without shell parsing' crates/operon-cli/src/main.rs
require_pattern 'job run .*argv' README.md
require_pattern 'argv\?: string\[\]' packages/sdk-js/src/index.ts
require_pattern 'argv: string\[\]' packages/sdk-js/src/generated/operon/runtime.ts
require_pattern 'job_run_request_preserves_argv_execution_fields' crates/operon-protocol/src/lib.rs
require_pattern 'argv_job_request_keeps_arguments_unescaped' crates/operon-cli/src/commands/job.rs

require_pattern 'load_job_logs' crates/operon-store/src/lib.rs
require_pattern 'job_log_buffers_from_persisted_logs' crates/operond/src/job_runtime.rs
require_pattern 'job_log_buffers_from_persisted_logs' crates/operond/src/main.rs
require_pattern 'persisted_job_logs_seed_bounded_log_buffers' crates/operond/src/job_runtime.rs

require_pattern 'service_check_audit_reason' crates/operond/src/service_forward.rs
require_pattern 'datagram response not verified' crates/operon-network/src/lib.rs
require_pattern 'UDP health is connection-setup-only' PROTOCOL.md
require_pattern 'UDP health records UDP socket setup semantics explicitly' docs/architecture/runtime-api.md
require_pattern 'service_check_audit_reason_names_udp_limited_reachability' crates/operond/src/service_forward.rs

require_pattern 'WorkspaceTraversalHardening' crates/operon-fs/src/lib.rs
require_pattern 'rejects_creating_path_below_symlink_parent_escape' crates/operon-fs/src/lib.rs

require_pattern 'advertise_lan=false' crates/operon-cli/src/commands/init.rs
require_pattern 'advertise_lan=true' crates/operon-cli/src/onboard.rs
require_pattern 'init_config_documents_loopback_lan_advertise_default' crates/operon-cli/src/commands/init.rs
require_pattern 'onboard_summary_documents_lan_advertise_default_for_daemon' crates/operon-cli/src/onboard.rs

cargo test -p operon-store --locked load_job_logs_reads_persisted_log_records_by_job
cargo test -p operond --locked persisted_job_logs_seed_bounded_log_buffers
cargo test -p operond --locked service_check_audit_reason
cargo test -p operon-protocol --locked job_run_request_preserves_argv_execution_fields
cargo test -p operon-protocol --locked protocol_version_matches_grpc_release_line
cargo test -p operon-cli --locked argv_job_request_keeps_arguments_unescaped
cargo test -p operon-cli --locked lan_advertise_default
cargo test -p operon-fs --locked traversal_hardening
cargo test -p operon-fs --locked symlink_parent_escape
pnpm --filter @operon/sdk test
bash scripts/verify-docs-help-skills-sync.sh

echo "v0.9.4 runtime hardening consolidation validation passed"
