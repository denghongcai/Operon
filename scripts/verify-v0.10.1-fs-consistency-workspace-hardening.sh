#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.10.1-fs-consistency-workspace-hardening.md
require_pattern 'Status: Completed' docs/plan/v0.10.1-fs-consistency-workspace-hardening.md
require_pattern 'v0.10.1 Filesystem Consistency and Workspace Hardening' docs/plan/development-phases.md
require_pattern 'No v0.10.1 work remains' docs/plan/development-phases.md

require_pattern 'message FsPrecondition' proto/operon/runtime.proto
require_pattern 'string version = 5' proto/operon/runtime.proto
require_pattern 'optional FsPrecondition precondition' proto/operon/runtime.proto
require_pattern 'optional string expected_version' proto/operon/runtime.proto
require_pattern 'bool require_absent' proto/operon/runtime.proto
require_pattern 'PROTOCOL_VERSION: &str = "v0.16.6"' crates/operon-protocol/src/lib.rs
require_pattern '"version": "0.16.6"' packages/sdk-js/package.json

require_pattern 'LinuxOpenat2ResolveBeneath' crates/operon-fs/src/lib.rs
require_pattern 'SYS_openat2' crates/operon-fs/src/lib.rs
require_pattern 'RESOLVE_BENEATH' crates/operon-fs/src/lib.rs
require_pattern 'check_precondition' crates/operond/src/fs_service.rs
require_pattern 'Status::failed_precondition' crates/operond/src/fs_service.rs
require_pattern 'expected_version' crates/operon-cli/src/cli_args.rs
require_pattern 'expected_version' packages/sdk-js/src/types.ts
require_pattern 'FsPrecondition' packages/sdk-js/src/index.ts
require_pattern 'Filesystem Concurrency and Preconditions' PROTOCOL.md
require_pattern 'FsPrecondition' docs/architecture/runtime-api.md

if ! output="$(cargo run -q -p operon-cli -- fs write --help 2>&1)"; then
  echo "operon fs write --help failed" >&2
  echo "$output" >&2
  exit 1
fi
grep -q -- '--expected-version' <<<"$output"

cargo test -p operon-protocol --locked fs_version_and_precondition_round_trip_through_grpc_shape
cargo test -p operond --locked fs_service::tests
cargo test -p operon-fs --locked traversal_hardening_strategy_is_explicit
cargo test -p operon-grpc-client --locked chunks_write_target_can_include_expected_version
if [[ "${OPERON_SKIP_SDK_TESTS:-}" == "1" ]]; then
  echo "skipping @operon/sdk tests; TypeScript CI already covers them"
else
  pnpm --filter @operon/sdk test
fi

echo "v0.10.1 filesystem consistency and workspace hardening validation passed"
