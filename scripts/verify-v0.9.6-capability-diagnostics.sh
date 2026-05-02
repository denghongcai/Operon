#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.9.6-capability-diagnostics.md
require_pattern 'Status: Completed' docs/plan/v0.9.6-capability-diagnostics.md
require_pattern 'v0.9.6 Capability Diagnostics' docs/plan/development-phases.md
require_pattern 'No v0.9.6 work remains' docs/plan/development-phases.md

require_pattern 'rpc ExplainCapability\(CapabilityDiagnosticRequest\) returns \(PolicyDecision\)' proto/operon/runtime.proto
require_pattern 'message CapabilityDiagnosticRequest' proto/operon/runtime.proto
require_pattern 'message PolicyDecision' proto/operon/runtime.proto
require_pattern 'PROTOCOL_VERSION: &str = "v0.9.6"' crates/operon-protocol/src/lib.rs

require_pattern 'struct CapabilityDiagnosticRequest' crates/operon-core/src/policy.rs
require_pattern 'policy_decision_round_trips_through_grpc_shape' crates/operon-protocol/src/lib.rs
require_pattern 'explain_capability_decision' crates/operond/src/capability_diagnostics.rs
require_pattern 'async fn explain_capability' crates/operond/src/main.rs
require_pattern 'CapabilityCommand::Explain' crates/operon-cli/src/main.rs
require_pattern 'pub async fn explain_capability' crates/operon-cli/src/grpc.rs
require_pattern 'explainCapability' packages/sdk-js/src/index.ts
require_pattern 'ExplainCapability' PROTOCOL.md docs/architecture/runtime-api.md README.md

cargo test -p operon-core --locked capability_diagnostic_request_serializes_optional_timeout
cargo test -p operon-protocol --locked policy_decision_round_trips_through_grpc_shape
cargo test -p operond --locked capability_diagnostics
cargo test -p operon-cli --locked capability_explain
pnpm --filter @operon/sdk test
bash scripts/verify-docs-help-skills-sync.sh

echo "v0.9.6 capability diagnostics validation passed"
