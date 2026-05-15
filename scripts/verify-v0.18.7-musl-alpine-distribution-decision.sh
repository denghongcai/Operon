#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.18.7-musl-alpine-distribution-decision.md
require_pattern 'Status: Completed' docs/plan/v0.18.7-musl-alpine-distribution-decision.md
require_pattern 'Phase 117: v0.18.7 musl / Alpine Distribution Decision' docs/plan/development-phases.md
require_pattern 'No v0.18.7 musl / Alpine distribution decision work remains' docs/plan/development-phases.md

require_file docs/decisions/musl-alpine-distribution.md
require_file scripts/assess-musl-alpine-distribution.sh
require_pattern 'Decision: keep glibc-only public Linux archives for now' docs/decisions/musl-alpine-distribution.md
require_pattern 'Alpine and musl-based distributions are unsupported by the prebuilt Linux archives' README.md docs/quality/release-install-usability.md
require_pattern 'scripts/assess-musl-alpine-distribution.sh' docs/quality/release-install-usability.md DEVELOPMENT.md AGENTS.md scripts/ci/run-validations.sh
require_pattern 'v0.18.7 musl / Alpine Distribution Decision Validation' scripts/ci/run-validations.sh
require_pattern 'alpine:' scripts/assess-musl-alpine-distribution.sh docs/decisions/musl-alpine-distribution.md
require_pattern 'x86_64-unknown-linux-musl' docs/decisions/musl-alpine-distribution.md

bash -n scripts/assess-musl-alpine-distribution.sh
bash -n scripts/verify-v0.18.7-musl-alpine-distribution-decision.sh

dry_run="$(scripts/assess-musl-alpine-distribution.sh --dry-run v0.16.6 denghongcai/Operon)"
for expected in \
  'tag=v0.16.6' \
  'image=alpine:' \
  'expected=unsupported-glibc-archive-on-musl'
do
  if ! grep -Fq "$expected" <<<"$dry_run"; then
    echo "musl/Alpine dry run missing expected output: $expected" >&2
    echo "$dry_run" >&2
    exit 1
  fi
done

echo "v0.18.7 musl / Alpine distribution decision validation passed"
