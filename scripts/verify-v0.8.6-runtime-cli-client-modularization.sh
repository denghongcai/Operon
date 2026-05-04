#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file crates/operon-grpc-client/Cargo.toml
require_file crates/operon-grpc-client/src/lib.rs
require_pattern 'crates/operon-grpc-client' Cargo.toml
require_pattern 'pub fn grpc_channel_uri' crates/operon-grpc-client/src/lib.rs
require_pattern 'pub async fn connect' crates/operon-grpc-client/src/lib.rs
require_pattern 'pub fn request_with_context' crates/operon-grpc-client/src/lib.rs
require_pattern 'pub fn chunk_write_requests' crates/operon-grpc-client/src/lib.rs
require_pattern 'pub fn chunk_stdin_requests' crates/operon-grpc-client/src/lib.rs
require_pattern 'operon_grpc_client::connect' crates/operon-cli/src/grpc.rs
require_pattern 'operon_grpc_client::request_with_context' crates/operon-cli/src/grpc.rs
require_pattern 'operon_grpc_client::request' crates/operon-mount/src/remote_client.rs

for module in audit capability config init exec mount node service trace; do
  require_file "crates/operon-cli/src/commands/${module}.rs"
  require_pattern "pub\\(crate\\) mod ${module};" crates/operon-cli/src/commands/mod.rs
done
require_pattern '^    Graph \{' crates/operon-cli/src/main.rs
require_pattern '^    Workflow \{' crates/operon-cli/src/main.rs
require_pattern 'struct FsReadOutputSummary' crates/operon-cli/src/commands/fs.rs
reject_pattern '^async fn exec_' crates/operon-cli/src/main.rs
reject_pattern '^async fn service_' crates/operon-cli/src/main.rs
reject_pattern '^fn audit_' crates/operon-cli/src/main.rs

for module in errors fuse_fs inode_table path session; do
  require_file "crates/operon-mount/src/${module}.rs"
  require_pattern "^mod ${module};" crates/operon-mount/src/lib.rs
done
require_file crates/operon-mount/src/mount_core.rs
require_file crates/operon-mount/src/remote_client.rs
require_pattern 'pub mod mount_core;' crates/operon-mount/src/lib.rs
require_pattern 'pub mod remote_client;' crates/operon-mount/src/lib.rs
require_pattern 'pub use mount_core::RemoteFs' crates/operon-mount/src/lib.rs
require_pattern 'pub trait RemoteFs' crates/operon-mount/src/mount_core.rs

for module in audit auth fs_service exec_runtime pagination service_forward state; do
  require_file "crates/operond/src/${module}.rs"
  require_pattern "^mod ${module};" crates/operond/src/main.rs
done
require_pattern 'pub\(crate\) fn start_exec' crates/operond/src/exec_runtime.rs
require_pattern 'pub\(crate\) fn append_exec_log' crates/operond/src/exec_runtime.rs
require_pattern 'pub\(crate\) fn finish_exec' crates/operond/src/exec_runtime.rs
require_file crates/operond/src/service_tcp_forward.rs
require_file crates/operond/src/service_datagram_forward.rs
require_pattern 'service_tcp_forward::service_tunnel_stream' crates/operond/src/service_forward.rs
require_pattern 'service_datagram_forward::service_datagram_tunnel_stream' crates/operond/src/service_forward.rs
require_pattern 'pub\(crate\) fn record_audit_capability' crates/operond/src/audit.rs
reject_pattern '^pub\(crate\) fn append_exec_log' crates/operond/src/main.rs
reject_pattern '^pub\(crate\) fn finish_exec' crates/operond/src/main.rs
reject_pattern '^pub\(crate\) fn service_tunnel_stream' crates/operond/src/main.rs

require_pattern 'statFs\(' packages/sdk-js/src/index.ts
require_pattern 'listFs\(' packages/sdk-js/src/index.ts
require_pattern 'runExec\(' packages/sdk-js/src/index.ts
require_pattern 'getExec\(' packages/sdk-js/src/index.ts
require_pattern 'cancelExec\(' packages/sdk-js/src/index.ts
require_pattern 'listCapabilities\(' packages/sdk-js/src/index.ts
require_pattern 'listAudit\(' packages/sdk-js/src/index.ts

cargo test -p operon-grpc-client --locked
cargo test -p operon-cli --locked
cargo test -p operon-mount --locked
cargo test -p operond --locked
if [[ "${OPERON_SKIP_SDK_TESTS:-}" == "1" ]]; then
  echo "skipping @operon/sdk tests; TypeScript CI already covers them"
else
  pnpm --filter @operon/sdk test
fi

cargo run --quiet -p operon-cli -- --help | rg -q '  graph '
cargo run --quiet -p operon-cli -- --help | rg -q '  workflow '
cargo run --quiet -p operon-cli -- graph --help | rg -q 'run'
cargo run --quiet -p operon-cli -- workflow --help | rg -q 'run'

echo "v0.8.6 runtime, CLI, and client modularization validation passed"
