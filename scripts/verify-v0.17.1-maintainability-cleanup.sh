#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.17.1-maintainability-cleanup.md
require_pattern 'Phase 104: v0.17.1 Maintainability Cleanup' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.17.1-maintainability-cleanup.md

require_file packages/sdk-js/src/grpc-mappers.ts
require_pattern 'from "./grpc-mappers"' packages/sdk-js/src/index.ts
require_pattern 'export function serviceTunnelReadableStream' packages/sdk-js/src/grpc-mappers.ts
require_pattern 'export async function\* mapGrpcExecSessionEvents' packages/sdk-js/src/grpc-mappers.ts
require_pattern 'export function fromGrpcExecRecord' packages/sdk-js/src/grpc-mappers.ts

require_file crates/operon-mount/src/windows_path.rs
require_file crates/operon-mount/src/windows_file_info.rs
require_file crates/operon-mount/src/windows_status.rs
require_pattern 'mod windows_path;' crates/operon-mount/src/lib.rs
require_pattern 'mod windows_file_info;' crates/operon-mount/src/lib.rs
require_pattern 'mod windows_status;' crates/operon-mount/src/lib.rs
require_pattern 'windows_name_to_remote_path' crates/operon-mount/src/windows_path.rs
require_pattern 'file_info_for_stat' crates/operon-mount/src/windows_file_info.rs
require_pattern 'ntstatus_for_error' crates/operon-mount/src/windows_status.rs

require_file crates/operond/src/daemon_cli.rs
require_pattern 'mod daemon_cli;' crates/operond/src/main.rs
require_pattern 'pub\(crate\) struct Args' crates/operond/src/daemon_cli.rs
require_pattern 'pub\(crate\) enum ServiceCommand' crates/operond/src/daemon_cli.rs

if [[ "${OPERON_SKIP_SDK_TESTS:-0}" == "1" ]]; then
  echo "skipping @operon/sdk tests; TypeScript CI already covers them"
else
  pnpm --dir packages/sdk-js typecheck
  pnpm --dir packages/sdk-js test
fi
cargo check -p operond --locked
if command -v rustup >/dev/null 2>&1; then
  rustup target add x86_64-pc-windows-gnu >/dev/null
fi
cargo check -p operond --target x86_64-pc-windows-gnu --tests --locked
cargo check -p operon-mount --target x86_64-pc-windows-gnu --tests --locked

echo "v0.17.1 maintainability cleanup validation passed"
