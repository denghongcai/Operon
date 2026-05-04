#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.13.2-windows-private-file-acl.md
require_pattern 'Status: Completed' docs/plan/v0.13.2-windows-private-file-acl.md
require_pattern 'Phase 86: v0.13.2 Windows Private File ACL Enforcement' docs/plan/development-phases.md
require_pattern 'No v0.13.2 Windows ACL work remains' docs/plan/development-phases.md

require_pattern 'WindowsAclSummary' crates/operon-cli/src/private_files.rs
require_pattern 'windows-acl-verified' crates/operon-cli/src/private_files.rs
require_pattern 'validate_windows_private_file_acl' crates/operon-cli/src/private_files.rs
require_pattern 'windows-acl-verified' crates/operon-cli/src/commands/doctor.rs
reject_pattern 'windows-acl-not-verified-warning' crates/operon-cli/src/commands/doctor.rs

require_pattern "if: runner.os == 'Windows'" .github/workflows/ci.yml
require_pattern 'cargo test -p operon-cli --locked windows_private_file_is_written_with_verified_acl' .github/workflows/ci.yml
require_pattern 'cargo test -p operon-cli --locked windows_private_file_acl_model' .github/workflows/ci.yml

require_pattern 'token and config private-file handling uses ACL-aware validation' README.md
require_pattern 'ACL-aware private-file' PROTOCOL.md
require_pattern 'Windows token and config private-file handling is ACL-aware' docs/architecture/runtime-api.md

cargo test -p operon-cli --locked windows_private_file_acl_model
cargo test -p operon-cli --locked private_file_security_model_is_platform_specific
cargo test -p operon-cli --locked platform_report_contains_operator_caveats

echo "v0.13.2 Windows private-file ACL validation passed"
