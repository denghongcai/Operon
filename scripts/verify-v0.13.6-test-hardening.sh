#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.13.6-test-hardening.md
require_pattern 'Status: Completed' docs/plan/v0.13.6-test-hardening.md
require_pattern 'Phase 90: v0.13.6 Test Hardening' docs/plan/development-phases.md
require_pattern 'No v0.13.6 test-hardening work remains' docs/plan/development-phases.md
python3 - <<'PY'
from pathlib import Path

text = Path("docs/plan/development-phases.md").read_text()
start = text.index("## Phase 90: v0.13.6 Test Hardening")
end = text.index("## Phase 91: v0.13.7 Mount Adapter Strategy")
block = text[start:end]
if "Status: Completed." not in block:
    raise SystemExit("v0.13.6 phase is not marked completed")
PY

require_pattern 'tcp service reachable' crates/operon-network/src/lib.rs
reject_pattern 'udp socket connected; datagram response not verified"\.to_string\(\)' crates/operon-network/src/lib.rs
require_pattern 'fn tcp_service_check_reports_tcp_reachability_on_success' crates/operon-network/src/lib.rs
require_pattern 'fn udp_service_check_reports_socket_connect_success' crates/operon-network/src/lib.rs

require_pattern 'DEFAULT_CONNECT_TIMEOUT' crates/operon-grpc-client/src/lib.rs
require_pattern 'fn connect_deadline_wraps_pending_connection_future' crates/operon-grpc-client/src/lib.rs
require_pattern 'fn chunks_non_empty_stdin_streams_at_configured_boundary' crates/operon-grpc-client/src/lib.rs
require_pattern 'fn chunks_non_empty_write_streams_at_configured_boundary' crates/operon-grpc-client/src/lib.rs

require_pattern 'fn maps_tonic_statuses_to_fuse_errno' crates/operon-mount/src/errors.rs
require_pattern 'fn lookup_child_fetches_remote_stat_and_caches_inode' crates/operon-mount/src/fuse_fs.rs
require_pattern 'fn lookup_child_rejects_escape_names_before_remote_stat' crates/operon-mount/src/fuse_fs.rs

require_pattern 'fn unknown_command_exits_nonzero_with_clap_error' crates/operon-cli/tests/cli_static_integration.rs
require_pattern 'fn malformed_config_exits_nonzero_for_config_loading_command' crates/operon-cli/tests/cli_static_integration.rs
require_pattern 'fn invalid_endpoint_scheme_is_rejected_before_rpc' crates/operon-cli/tests/cli_static_integration.rs
require_pattern 'fn daemon_onboard_plan_writes_private_token_file_and_references_it' crates/operon-cli/src/onboard.rs

reject_pattern 'remove_dir_all\(' crates/operon-cli
reject_pattern 'remove_dir_all\(' crates/operon-store
reject_pattern 'remove_dir_all\(' crates/operond

require_pattern 'Status: Updated for v0.13.6' docs/quality/test-coverage-audit.md
require_pattern 'negative-path' docs/quality/test-coverage-audit.md
require_pattern 'TempDir' docs/quality/test-coverage-audit.md

cargo test -p operon-network --locked
cargo test -p operon-grpc-client --locked
cargo test -p operon-mount --locked --lib
cargo test -p operon-store --locked
cargo test -p operon-cli --locked
cargo test -p operond --locked

echo "v0.13.6 test hardening validation passed"
