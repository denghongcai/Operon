#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.13.7-mount-adapter-strategy.md
require_pattern 'Status: Completed' docs/plan/v0.13.7-mount-adapter-strategy.md
require_pattern 'macFUSE 5.2.0' docs/plan/v0.13.7-mount-adapter-strategy.md
require_pattern 'WinFsp 2025 / v2.1' docs/plan/v0.13.7-mount-adapter-strategy.md
require_pattern 'backend=fskit' docs/plan/v0.13.7-mount-adapter-strategy.md
require_pattern 'reduced-security' docs/plan/v0.13.7-mount-adapter-strategy.md
require_pattern 'prefer WinFsp.s native API' docs/plan/v0.13.7-mount-adapter-strategy.md
require_pattern 'Linux-only for supported live mounts before v1.0' docs/plan/v0.13.7-mount-adapter-strategy.md
require_pattern 'shared mount-core boundary' docs/plan/v0.13.7-mount-adapter-strategy.md

require_file docs/plan/v0.13.8-mount-core-boundary.md
require_pattern 'Status: Completed' docs/plan/v0.13.8-mount-core-boundary.md
require_pattern 'Platform-neutral mount behavior' docs/plan/v0.13.8-mount-core-boundary.md

require_pattern 'Phase 91: v0.13.7 Mount Adapter Strategy' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/development-phases.md
require_pattern 'Phase 92: v0.13.8 Mount Core Boundary' docs/plan/development-phases.md
require_pattern 'Linux FUSE, macOS macFUSE, and Windows WinFsp' docs/architecture/technology-and-protocol-decisions.md
require_pattern 'Linux FUSE, macOS macFUSE, and Windows WinFsp' README.md
require_pattern 'superseded by' docs/plan/v0.13.7-mount-adapter-strategy.md
require_pattern 'Completed mount-core boundary milestone: v0.13.8' AGENTS.md

bash scripts/verify-docs-help-skills-sync.sh

echo "v0.13.7 mount adapter strategy validation passed"
