#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.16.2-sdk-maintainability-cleanup.md
require_pattern 'Phase 99: v0.16.2 SDK Maintainability Cleanup' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.16.2-sdk-maintainability-cleanup.md
require_file packages/sdk-js/src/transport.ts
require_file packages/sdk-js/src/grpc-requests.ts
require_pattern 'from "./transport"' packages/sdk-js/src/index.ts
require_pattern 'from "./grpc-requests"' packages/sdk-js/src/index.ts
require_pattern 'export function grpcOptions' packages/sdk-js/src/transport.ts
require_pattern 'export async function bodyToBytes' packages/sdk-js/src/transport.ts
require_pattern 'export async function\* grpcFileChunksFromBody' packages/sdk-js/src/grpc-requests.ts
require_pattern 'export async function\* grpcExecSessionRequests' packages/sdk-js/src/grpc-requests.ts

pnpm --dir packages/sdk-js typecheck
pnpm --dir packages/sdk-js test

echo "v0.16.2 SDK maintainability cleanup validation passed"
