#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_pattern 'Phase 63: Post-v0.9 Discovery UX' docs/plan/development-phases.md
require_pattern 'scripts/verify-post-v0.9-discovery-ux.sh' DEVELOPMENT.md
require_pattern 'scripts/verify-post-v0.9-discovery-ux.sh' scripts/ci/run-validations.sh
require_pattern 'write_discovered_config_refuses_conflicting_existing_endpoint' crates/operon-cli/src/commands/node.rs
require_pattern 'check_discovered_health' crates/operon-cli/src/commands/node.rs

if ! cargo run -q -p operon-cli -- node discover --help | grep -q -- '--check-health'; then
  echo 'node discover help is missing --check-health' >&2
  exit 1
fi

cargo test -p operon-cli --locked write_discovered_config_refuses_conflicting_existing_endpoint
cargo test -p operon-cli --locked discovery_rows_include_health_status_when_requested
bash scripts/verify-v0.9-endpoint-model.sh

echo "post-v0.9 discovery UX validation passed"
