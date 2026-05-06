#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.16.1-generic-mount-release-naming.md
require_pattern 'Phase 98: v0.16.1 Generic Mount and Release Naming Cleanup' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.16.1-generic-mount-release-naming.md
require_file .github/workflows/live-mount-smoke.yml
require_pattern 'Cross-Platform Live Mount Smoke' .github/workflows/live-mount-smoke.yml
require_pattern 'scripts/install-macos-fuse-t.sh' .github/workflows/release-draft.yml
require_pattern 'scripts/install-macos-fuse-t.sh' .github/workflows/live-mount-smoke.yml
require_pattern 'scripts/preflight-macos-fuse-t-host.sh' .github/workflows/live-mount-smoke.yml
require_pattern 'scripts/smoke-macos-live-mount.sh' .github/workflows/live-mount-smoke.yml
require_pattern 'scripts/smoke-windows-live-mount.ps1' .github/workflows/live-mount-smoke.yml
require_pattern 'exec scripts/install-macos-fuse-t.sh "\$@"' scripts/install-v0.14-macos-fuse-t.sh
require_pattern 'exec scripts/preflight-macos-fuse-t-host.sh "\$@"' scripts/preflight-v0.14-macos-fuse-t-host.sh
require_pattern 'exec scripts/smoke-macos-live-mount.sh "\$@"' scripts/smoke-v0.14-macos-live-mount.sh
require_pattern 'smoke-windows-live-mount.ps1' scripts/smoke-v0.14-windows-live-mount.ps1

bash -n scripts/install-macos-fuse-t.sh
bash -n scripts/preflight-macos-fuse-t-host.sh
bash -n scripts/smoke-macos-live-mount.sh
bash -n scripts/smoke-macos-fuse-zip-probe.sh
bash -n scripts/smoke-macos-fuser-hello-probe.sh
bash -n scripts/smoke-macos-libfuse-lowlevel-hello-probe.sh
bash -n scripts/install-v0.14-macos-fuse-t.sh
bash -n scripts/preflight-v0.14-macos-fuse-t-host.sh
bash -n scripts/smoke-v0.14-macos-live-mount.sh

echo "v0.16.1 generic mount/release naming validation passed"
