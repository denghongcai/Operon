#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.18.2-sdk-api-boundary-cleanup.md
require_pattern 'Phase 111: v0.18.2 SDK API Boundary Cleanup' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.18.2-sdk-api-boundary-cleanup.md
require_pattern 'No v0.18.2 SDK API boundary cleanup work remains' docs/plan/development-phases.md

require_file packages/sdk-js/src/types.ts
require_pattern 'export type NodeEndpoint' packages/sdk-js/src/types.ts
require_pattern 'export type ExecRecord' packages/sdk-js/src/types.ts
require_pattern 'export type ServiceDatagramTunnelEvent' packages/sdk-js/src/types.ts
require_pattern 'from "./types"' packages/sdk-js/src/index.ts
require_pattern 'export type \{' packages/sdk-js/src/index.ts
require_pattern 'export class OperonClient' packages/sdk-js/src/index.ts
require_pattern 'from "./types"' packages/sdk-js/src/grpc-mappers.ts
require_pattern 'from "./types"' packages/sdk-js/src/grpc-requests.ts
require_pattern 'from "./types"' packages/sdk-js/src/transport.ts
reject_pattern 'from "./index"' packages/sdk-js/src/grpc-mappers.ts
reject_pattern 'from "./index"' packages/sdk-js/src/grpc-requests.ts
reject_pattern 'from "./index"' packages/sdk-js/src/transport.ts
reject_pattern 'export type NodeEndpoint =' packages/sdk-js/src/index.ts
reject_pattern 'export type ExecRecord =' packages/sdk-js/src/index.ts

if [[ "${OPERON_SKIP_SDK_TESTS:-0}" == "1" ]]; then
  echo "skipping @operon/sdk typecheck/tests/build; TypeScript CI already covers SDK execution"
else
  pnpm --filter @operon/sdk typecheck
  pnpm --filter @operon/sdk test
  pnpm --filter @operon/sdk build
fi

echo "v0.18.2 SDK API boundary cleanup validation passed"
