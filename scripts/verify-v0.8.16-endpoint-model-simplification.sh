#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.8.16-endpoint-model-simplification.md
require_pattern 'pub struct NodeEndpoint' crates/operon-config/src/lib.rs
require_pattern '#\[serde\(deny_unknown_fields\)\]' crates/operon-config/src/lib.rs
reject_pattern 'NetworkProviderKind' crates/operon-config/src/lib.rs
reject_pattern 'provider:' crates/operon-cli/src/commands/init.rs
reject_pattern 'provider:' README.md
reject_pattern 'ProviderCommand' crates/operon-cli/src/main.rs
reject_pattern 'provider --help' scripts/verify-v0.8-agent-skills.sh
reject_pattern ' --provider ' README.md
require_pattern 'operon node discover --timeout-secs 3' README.md
require_pattern 'v0.8.16 Endpoint Model Simplification' docs/plan/development-phases.md

if [[ -e crates/operon-cli/src/commands/provider.rs ]]; then
  echo "provider command module should not exist"
  exit 1
fi

cargo test -p operon-config --locked
cargo test -p operon-cli --locked
cargo test -p operon-network --locked

echo "v0.8.16 endpoint model simplification validation passed"
