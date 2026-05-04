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

require_pattern 'PROTOCOL_VERSION: &str = "v0.14.0"' crates/operon-protocol/src/lib.rs
require_pattern 'repeated string argv = 6' proto/operon/runtime.proto
require_pattern 'argv: value.argv' crates/operon-protocol/src/lib.rs
require_pattern 'Execute CLI words as argv without shell parsing' crates/operon-cli/src/main.rs
require_pattern 'exec run .*argv' README.md
require_pattern 'argv\?: string\[\]' packages/sdk-js/src/index.ts
require_pattern 'argv: string\[\]' packages/sdk-js/src/generated/operon/runtime.ts
require_pattern 'exec_run_request_preserves_argv_execution_fields' crates/operon-protocol/src/lib.rs
require_pattern 'argv_exec_request_keeps_arguments_unescaped' crates/operon-cli/src/commands/exec_args.rs

require_pattern 'load_exec_logs' crates/operon-store/src/lib.rs
require_pattern 'exec_log_buffers_from_persisted_logs' crates/operond/src/exec_runtime.rs
require_pattern 'exec_log_buffers_from_persisted_logs' crates/operond/src/main.rs
require_pattern 'persisted_exec_logs_seed_bounded_log_buffers' crates/operond/src/exec_runtime.rs

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

cargo test -p operon-store --locked load_exec_logs_reads_persisted_log_records_by_exec
cargo test -p operond --locked persisted_exec_logs_seed_bounded_log_buffers
cargo test -p operond --locked service_check_audit_reason
cargo test -p operon-protocol --locked exec_run_request_preserves_argv_execution_fields
cargo test -p operon-protocol --locked protocol_version_matches_grpc_release_line
cargo test -p operon-cli --locked argv_exec_request_keeps_arguments_unescaped
cargo test -p operon-cli --locked lan_advertise_default
cargo test -p operon-fs --locked traversal_hardening
cargo test -p operon-fs --locked symlink_parent_escape
if [[ "${OPERON_SKIP_SDK_TESTS:-}" == "1" ]]; then
  echo "skipping @operon/sdk tests; TypeScript CI already covers them"
else
  pnpm --filter @operon/sdk test
fi
bash scripts/verify-docs-help-skills-sync.sh

echo "v0.9.4 runtime hardening consolidation validation passed"
