#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.8.9-service-tunnel-boundary-cleanup.md
require_file crates/operond/src/service_forward.rs
require_pattern 'pub\(crate\) type ServiceTunnelStream' crates/operond/src/service_forward.rs
require_pattern 'pub\(crate\) type ServiceDatagramTunnelStream' crates/operond/src/service_forward.rs
require_pattern 'pub\(crate\) async fn open_service_tunnel' crates/operond/src/service_forward.rs
require_pattern 'pub\(crate\) async fn open_service_datagram_tunnel' crates/operond/src/service_forward.rs
require_pattern 'open_service_tunnel\(&self.state, input\)' crates/operond/src/main.rs
require_pattern 'open_service_datagram_tunnel\(&self.state, input\)' crates/operond/src/main.rs
reject_pattern 'service tunnel target metadata is required' crates/operond/src/main.rs
reject_pattern 'service datagram tunnel target metadata is required' crates/operond/src/main.rs
reject_pattern 'TcpStream::connect' crates/operond/src/main.rs
require_pattern 'v0.8.9 Service Tunnel Boundary Cleanup' docs/plan/development-phases.md

cargo test -p operond --locked

echo "v0.8.9 service tunnel boundary cleanup validation passed"
