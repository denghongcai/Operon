#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_pattern 'validate_private_file_permissions' crates/operon-config/src/lib.rs crates/operond/src/daemon_state.rs crates/operon-cli/src/commands/doctor.rs
require_pattern 'daemon_state_rejects_broad_token_file_permissions' crates/operond/src/daemon_state.rs
require_pattern 'daemon_state_rejects_empty_auth_for_non_loopback_bind' crates/operond/src/daemon_state.rs
require_pattern 'windows_acl_summary_rejects_public_file_access' crates/operon-config/src/lib.rs
require_pattern 'security_diagnostics' crates/operon-cli/src/commands/doctor.rs
require_pattern 'daemon-token-file-private' crates/operon-cli/src/commands/doctor.rs
require_pattern 'client-node-\{node_id\}-token-file-private' crates/operon-cli/src/commands/doctor.rs
require_pattern 'secrets-file-private' crates/operon-cli/src/commands/doctor.rs
require_pattern 'ServicePermissions::default\(\)' crates/operond/src/service_forward.rs crates/operond/src/main.rs
require_pattern '[Ss]ervice permissions are default-deny' PROTOCOL.md README.md docs/architecture/runtime-api.md
require_pattern 'Security hardening validation' scripts/ci/run-validations.sh
require_pattern 'docs/quality/security-hardening.md' README.md docs/quality/release-install-usability.md

bash -n scripts/verify-security-hardening.sh
cargo test -p operon-config --locked validates_private_file_permissions_on_unix
cargo test -p operon-config --locked windows_acl_summary_rejects_public_file_access
cargo test -p operond --locked daemon_state_rejects_broad_token_file_permissions
cargo test -p operond --locked daemon_state_rejects_empty_auth_for_non_loopback_bind
cargo test -p operond --locked service_authorization_decision_names_reason_codes
cargo test -p operon-cli --locked security_diagnostics

echo "security hardening validation passed"
