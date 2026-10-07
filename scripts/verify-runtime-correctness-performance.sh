#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo test -p operon-store --locked
cargo test -p operond --locked fs_service::tests
cargo test -p operond --locked runtime::tests
cargo test -p operon-mount --locked
cargo test -p operon-cli --locked grpc_pagination_tests
# Grouped CI runs SDK checks in its prerequisite TypeScript job; validation
# runners intentionally do not install pnpm. Local runs include them by default.
if [[ "${OPERON_SKIP_SDK_TESTS:-0}" != "1" ]]; then
  pnpm --filter @operon/sdk typecheck
  pnpm --filter @operon/sdk test
fi
echo "Runtime correctness and filesystem efficiency validation passed"
