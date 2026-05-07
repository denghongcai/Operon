#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.18.3-sdk-public-api-contract-hardening.md
require_pattern 'Phase 112: v0.18.3 SDK Public API Contract Hardening' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.18.3-sdk-public-api-contract-hardening.md
require_pattern 'No v0.18.3 SDK public API contract hardening work remains' docs/plan/development-phases.md

require_file packages/sdk-js/api-contract/public-api-contract.ts
require_file packages/sdk-js/api-contract/public-api-exports.txt
require_file packages/sdk-js/tsconfig.public-api.json
require_pattern 'tsconfig.public-api.json --noEmit' packages/sdk-js/package.json
require_pattern 'new OperonClient' packages/sdk-js/api-contract/public-api-contract.ts
require_pattern 'type ClientContract' packages/sdk-js/api-contract/public-api-contract.ts
require_pattern 'OperonRuntimeDefinition' packages/sdk-js/api-contract/public-api-contract.ts
require_pattern 'type OperonRuntimeClient' packages/sdk-js/api-contract/public-api-contract.ts
require_pattern 'type ServiceDatagramTunnelEvent' packages/sdk-js/api-contract/public-api-contract.ts
require_pattern 'type ExecSessionEvent' packages/sdk-js/api-contract/public-api-contract.ts
require_pattern 'type AuditLog' packages/sdk-js/api-contract/public-api-contract.ts

unexpected_exports="$(
  rg -n '^export ' packages/sdk-js/src/index.ts |
    rg -v 'export type \{' |
    rg -v 'export class OperonClient' |
    rg -v 'export type \{ OperonRuntimeClient \};' |
    rg -v 'export \{ OperonRuntimeDefinition \};' || true
)"
if [[ -n "$unexpected_exports" ]]; then
  echo "unexpected public export form in packages/sdk-js/src/index.ts:" >&2
  echo "$unexpected_exports" >&2
  exit 1
fi

actual_exports="$(
  awk '
    /^export type \{$/ { in_block = 1; next }
    in_block && /^\} from "\.\/types";/ { in_block = 0; next }
    in_block {
      gsub(/[ ,]/, "")
      if (length($0) > 0) print $0
    }
    END {
      print "OperonClient"
      print "OperonRuntimeClient"
      print "OperonRuntimeDefinition"
    }
  ' packages/sdk-js/src/index.ts | sort
)"
expected_exports="$(sort packages/sdk-js/api-contract/public-api-exports.txt)"
if [[ "$actual_exports" != "$expected_exports" ]]; then
  echo "SDK public export list differs from packages/sdk-js/api-contract/public-api-exports.txt" >&2
  diff -u <(printf '%s\n' "$expected_exports") <(printf '%s\n' "$actual_exports") >&2 || true
  exit 1
fi

if [[ "${OPERON_SKIP_SDK_TESTS:-0}" == "1" ]]; then
  echo "skipping @operon/sdk public API typecheck; TypeScript CI runs package typecheck"
else
  pnpm --dir packages/sdk-js exec tsc --project tsconfig.public-api.json --noEmit
fi

echo "v0.18.3 SDK public API contract hardening validation passed"
