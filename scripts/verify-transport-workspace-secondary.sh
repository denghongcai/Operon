#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
cargo test -p operon-config --locked
cargo test -p operon-grpc-client --locked
cargo test -p operon-fs --locked
cargo test -p operon-store --locked
cargo test -p operond --locked
cargo test -p operon-cli --locked grpc_pagination_tests
cargo test -p operon-cli --locked target::tests
cargo test -p operon-mount --locked errors::tests
cargo test -p operon-mount --locked remote_client::tests
if [[ "${OPERON_SKIP_SDK_TESTS:-0}" != "1" ]]; then
  pnpm --filter @operon/sdk typecheck
  pnpm --filter @operon/sdk test
fi
echo "Transport, workspace and secondary hardening validation passed"
