#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.10.5-maintainability-cleanup.md
require_pattern 'Status: Completed' docs/plan/v0.10.5-maintainability-cleanup.md
require_pattern 'v0.10.5 Maintainability Cleanup' docs/plan/development-phases.md
require_pattern 'No v0.10.5 work remains' docs/plan/development-phases.md

require_file crates/operond/src/service_tcp_forward.rs
require_file crates/operond/src/service_datagram_forward.rs
require_file crates/operon-cli/src/grpc_service.rs
require_pattern 'mod service_tcp_forward' crates/operond/src/main.rs
require_pattern 'mod service_datagram_forward' crates/operond/src/main.rs
require_pattern 'mod grpc_service' crates/operon-cli/src/main.rs
require_pattern 'service_tcp_forward::service_tunnel_stream' crates/operond/src/service_forward.rs
require_pattern 'service_datagram_forward::service_datagram_tunnel_stream' crates/operond/src/service_forward.rs
reject_pattern 'struct ServiceDatagramPeerSession' crates/operond/src/service_forward.rs
reject_pattern 'pub async fn forward_service' crates/operon-cli/src/grpc.rs
reject_pattern 'pub async fn forward_service_datagram' crates/operon-cli/src/grpc.rs

cargo test -p operond --locked service_forward
cargo test -p operon-cli --locked grpc_service
scripts/verify-v0.7-service-forwarding.sh

echo "v0.10.5 maintainability cleanup validation passed"
