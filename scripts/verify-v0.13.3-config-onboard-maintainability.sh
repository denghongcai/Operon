#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.13.3-config-onboard-maintainability.md
require_pattern 'Status: Completed' docs/plan/v0.13.3-config-onboard-maintainability.md
require_pattern 'Phase 87: v0.13.3 Config and Onboard Maintainability Cleanup' docs/plan/development-phases.md
require_pattern 'No v0.13.3 config/onboard cleanup work remains' docs/plan/development-phases.md

require_file crates/operon-cli/src/commands/config/explain.rs
require_pattern 'mod explain;' crates/operon-cli/src/commands/config.rs
require_pattern 'pub\(crate\) use explain::explain' crates/operon-cli/src/commands/config.rs
require_pattern 'fn build_config_explain' crates/operon-cli/src/commands/config/explain.rs
require_pattern 'fn print_config_explain' crates/operon-cli/src/commands/config/explain.rs

require_file crates/operon-cli/src/onboard/plan.rs
require_file crates/operon-cli/src/onboard/render.rs
require_file crates/operon-cli/src/onboard/write.rs
require_pattern 'mod plan;' crates/operon-cli/src/onboard.rs
require_pattern 'mod render;' crates/operon-cli/src/onboard.rs
require_pattern 'mod write;' crates/operon-cli/src/onboard.rs
require_pattern 'pub\(super\) fn build_onboard_plan' crates/operon-cli/src/onboard/plan.rs
require_pattern 'pub\(super\) fn print_onboard_plan' crates/operon-cli/src/onboard/render.rs
require_pattern 'pub\(super\) fn write_plan_files' crates/operon-cli/src/onboard/write.rs

cargo test -p operon-cli --locked config_explain_summarizes_unified_config_without_secret_values
cargo test -p operon-cli --locked daemon_onboard_plan_writes_private_token_file_and_references_it
cargo test -p operon-cli --locked onboard_summary_includes_shell_completion_commands

echo "v0.13.3 config/onboard maintainability validation passed"
