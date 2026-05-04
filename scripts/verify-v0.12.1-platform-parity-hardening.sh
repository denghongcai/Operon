#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.12.1-platform-parity-hardening.md
require_pattern 'Status: Completed' docs/plan/v0.12.1-platform-parity-hardening.md
require_pattern 'Phase 79: v0.12.1 Platform Parity Hardening' docs/plan/development-phases.md
require_pattern 'No v0.12.1 work remains' docs/plan/development-phases.md

require_pattern 'private_file_security_model' crates/operon-cli/src/private_files.rs
require_pattern 'windows-acl-warning' crates/operon-cli/src/private_files.rs
require_pattern 'windows-acl-not-verified-warning' crates/operon-cli/src/commands/doctor.rs
require_pattern 'exec_cancellation_guarantee' crates/operond/src/exec_runtime.rs
require_pattern 'job-object-process-tree' crates/operond/src/exec_runtime.rs
require_pattern 'process-group' crates/operond/src/exec_runtime.rs
require_pattern 'exec_session_portable_pty_smoke_outputs_and_exits' crates/operond/src/exec_session.rs
require_pattern 'portable-pty-smoke-validated' crates/operon-cli/src/commands/doctor.rs
require_pattern 'windows-exec-session-unsupported' crates/operon-cli/src/commands/doctor.rs
require_pattern 'service forwarding depends on local and remote firewall policy' crates/operon-cli/src/commands/doctor.rs

require_pattern "if: runner.os != 'Windows'" .github/workflows/ci.yml
require_pattern 'cargo test -p operond --locked exec_session_portable_pty_smoke_outputs_and_exits' .github/workflows/ci.yml
require_pattern "if: runner.os == 'Windows'" .github/workflows/ci.yml
require_pattern 'cargo test -p operond --locked windows_exec_session_is_explicitly_unsupported' .github/workflows/ci.yml
require_pattern 'cargo test -p operond --locked exec_cancellation_guarantee_matches_platform' .github/workflows/ci.yml
require_pattern 'cargo test -p operon-cli --locked private_file_security_model_is_platform_specific' .github/workflows/ci.yml
require_pattern 'v0.12.1 Platform Parity Hardening Validation' scripts/ci/run-validations.sh

require_pattern 'Windows' README.md
require_pattern 'private token/config handling currently reports an ACL warning' README.md
require_pattern 'Windows non-interactive exec' PROTOCOL.md
require_pattern 'Job Object process-tree termination' PROTOCOL.md
require_pattern 'Windows interactive exec sessions are explicitly unsupported' docs/architecture/runtime-api.md

cargo test -p operond --locked exec_cancellation_guarantee_matches_platform
cargo test -p operond --locked exec_session_portable_pty_smoke_outputs_and_exits
cargo test -p operon-cli --locked private_file_security_model_is_platform_specific
cargo test -p operon-cli --locked platform_report_contains_operator_caveats

echo "v0.12.1 platform parity hardening validation passed"
