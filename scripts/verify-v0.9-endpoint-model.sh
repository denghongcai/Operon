#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.9-acceptance.md
require_pattern 'Status: Completed' docs/plan/v0.9-acceptance.md
require_pattern 'v0.9 Endpoint Model Acceptance' docs/plan/development-phases.md
require_pattern 'scripts/verify-v0.9-endpoint-model.sh' DEVELOPMENT.md
require_pattern 'scripts/verify-v0.9-endpoint-model.sh' scripts/ci/run-validations.sh

reject_pattern 'provider:' examples/config.yaml
reject_pattern 'provider-specific API adapters' README.md
reject_pattern 'provider discovery' README.md

require_pattern 'loads_unified_config_with_client_nodes' crates/operon-config/src/lib.rs
require_pattern 'reports_unknown_fields_without_blocking_config_parse' crates/operon-config/src/lib.rs
require_pattern 'resolved_mdns_record_yields_endpoint_candidate_without_provider_metadata' crates/operon-network/src/lib.rs
require_pattern 'write_discovered_config_exports_endpoint_only_nodes_without_policy' crates/operon-cli/src/commands/node.rs

cargo test -p operon-config --locked loads_unified_config_with_client_nodes
cargo test -p operon-config --locked reports_unknown_fields_without_blocking_config_parse
cargo test -p operon-network --locked resolved_mdns_record_yields_endpoint_candidate_without_provider_metadata
cargo test -p operon-cli --locked write_discovered_config_exports_endpoint_only_nodes_without_policy
bash scripts/verify-docs-help-skills-sync.sh

echo "v0.9 endpoint model validation passed"
