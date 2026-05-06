#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.16-mount-runtime-ux-hardening.md

require_pattern 'Phase 97: v0.16 Mount Runtime UX Hardening' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.16-mount-runtime-ux-hardening.md
require_pattern 'mount_runtime: String' crates/operon-cli/src/commands/doctor.rs
require_pattern 'mount_hint: String' crates/operon-cli/src/commands/doctor.rs
require_pattern 'linux-fuse-runtime-missing' crates/operon-cli/src/commands/doctor.rs
require_pattern 'linux-fuse-helper-missing' crates/operon-cli/src/commands/doctor.rs
require_pattern 'macos-fuse-t-runtime-missing' crates/operon-cli/src/commands/doctor.rs
require_pattern 'windows-winfsp-runtime-missing' crates/operon-cli/src/commands/doctor.rs
require_pattern 'mount_runtime=\{\}' crates/operon-cli/src/commands/doctor.rs
require_pattern 'mount_hint=\{\}' crates/operon-cli/src/commands/doctor.rs
require_pattern 'mount_runtime_hint\(\)' crates/operon-cli/src/commands/mount.rs
require_pattern 'brew install macos-fuse-t/homebrew-cask/fuse-t' crates/operon-cli/src/commands/mount.rs
require_pattern 'WinFsp runtime' crates/operon-cli/src/commands/mount.rs
require_pattern 'runtime status and install hints' README.md
require_pattern 'reports mount runtime status' PROTOCOL.md

cargo test -p operon-cli --locked platform_report_contains_operator_caveats

echo "v0.16 mount runtime UX hardening validation passed"
