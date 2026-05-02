#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.8.17-config-unknown-field-warnings.md
require_file crates/operon-config/src/warnings.rs
require_pattern 'pub use warnings::ConfigWarning' crates/operon-config/src/lib.rs
require_pattern 'pub struct ConfigWarning' crates/operon-config/src/warnings.rs
require_pattern 'from_str_with_warnings' crates/operon-config/src/lib.rs
require_pattern 'collect_unknown_config_fields' crates/operon-config/src/warnings.rs
require_pattern 'warning: unknown config field' crates/operon-config/src/lib.rs
require_pattern 'reports_unknown_fields_without_blocking_config_parse' crates/operon-config/src/lib.rs
require_pattern 'config_unknown_fields_warn_without_blocking_command' crates/operon-cli/tests/cli_static_integration.rs
require_pattern 'v0.8.17 Config Unknown Field Warnings' docs/plan/development-phases.md

cargo test -p operon-config reports_unknown_fields_without_blocking_config_parse --locked
cargo test -p operon-cli config_unknown_fields_warn_without_blocking_command --locked

echo "v0.8.17 config unknown field warnings validation passed"
