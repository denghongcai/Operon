#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.9.5-policy-language-hardening.md
require_pattern 'Status: Completed' docs/plan/v0.9.5-policy-language-hardening.md
require_pattern 'v0.9.5 Policy Language Hardening' docs/plan/development-phases.md
require_pattern 'No v0.9.5 work remains' docs/plan/development-phases.md

require_pattern 'struct PolicyDecision' crates/operon-core/src/policy.rs
require_pattern 'enum PolicyReasonCode' crates/operon-core/src/policy.rs
require_pattern 'fs-mount-not-allowed' crates/operon-core/src/policy.rs
require_pattern 'service-action-denied' crates/operon-core/src/policy.rs
require_pattern 'policy_decision_serializes_stable_reason_code' crates/operon-core/src/policy.rs

require_pattern 'authorize_fs_decision' crates/operon-fs/src/lib.rs
require_pattern 'authorize_job_decision' crates/operon-process/src/lib.rs
require_pattern 'resolve_job_secrets_decision' crates/operon-process/src/lib.rs
require_pattern 'authorize_service_decision' crates/operond/src/service_forward.rs
require_pattern 'record_policy_decision' crates/operond/src/audit.rs
require_pattern 'policy_decision_audit_reason_includes_reason_code' crates/operond/src/audit.rs
require_pattern 'denied_job_policy_audit_uses_reason_code' crates/operond/src/main.rs

require_pattern 'effective_grants' crates/operon-cli/src/commands/config.rs
require_pattern 'effective grants' crates/operon-cli/src/commands/config.rs
require_pattern 'policy vocabulary' DEVELOPMENT.md
require_pattern 'policy decision vocabulary' PROTOCOL.md
require_pattern 'Policy decisions' docs/architecture/runtime-api.md

cargo test -p operon-core --locked policy_decision
cargo test -p operon-core --locked policy_reason_code_has_stable_string_form
cargo test -p operon-fs --locked fs_authorization_decision
cargo test -p operon-process --locked authorization_decision
cargo test -p operond --locked service_authorization_decision
cargo test -p operond --locked policy_decision_audit_reason
cargo test -p operond --locked denied_job_policy_audit_uses_reason_code
cargo test -p operon-cli --locked config_explain_summarizes_unified_config_without_secret_values
bash scripts/verify-docs-help-skills-sync.sh

echo "v0.9.5 policy language hardening validation passed"
