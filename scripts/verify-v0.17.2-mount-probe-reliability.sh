#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.17.2-mount-probe-reliability.md
require_pattern 'Phase 105: v0.17.2 Mount Probe Reliability Cleanup' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.17.2-mount-probe-reliability.md

require_pattern 'OPERON_FUSER_HELLO_DIAGNOSTIC_ONLY: \$\{\{ inputs.platform == '\''all'\'' && '\''1'\'' \|\| '\''0'\'' \}\}' .github/workflows/live-mount-smoke.yml
require_pattern 'diagnostic-only for platform=all' .github/workflows/live-mount-smoke.yml
require_pattern 'platform=macos-fuser-hello' .github/workflows/live-mount-smoke.yml
require_pattern 'exit "\$status"' .github/workflows/live-mount-smoke.yml

require_pattern 'macOS FUSE-T Live Mount \(hosted\)' .github/workflows/live-mount-smoke.yml
require_pattern 'Windows WinFsp Live Mount' .github/workflows/live-mount-smoke.yml
require_pattern 'macOS FUSE-T fuse-zip Probe \(hosted\)' .github/workflows/live-mount-smoke.yml
require_pattern 'macOS FUSE-T libfuse Low-Level Hello Probe \(hosted\)' .github/workflows/live-mount-smoke.yml

echo "v0.17.2 mount probe reliability validation passed"
