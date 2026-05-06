#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.16.4-mount-runtime-preflight-ux.md
require_pattern 'Phase 101: v0.16.4 Mount Runtime Preflight UX' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.16.4-mount-runtime-preflight-ux.md
require_pattern 'mount_runtime: bool' crates/operon-cli/src/cli_args.rs
require_pattern 'doctor --mount-runtime' README.md
require_pattern 'operon doctor --mount-runtime' PROTOCOL.md
require_pattern 'mount_runtime_ready: bool' crates/operon-cli/src/commands/doctor.rs
require_pattern 'mount runtime preflight failed' crates/operon-cli/src/commands/mount.rs
require_pattern 'if !runtime.ready' crates/operon-cli/src/commands/mount.rs
require_pattern 'mount_runtime_ready=\{\}' crates/operon-cli/src/commands/doctor.rs

cargo test -p operon-cli --locked mount_runtime
cargo test -p operon-cli --locked clap_model_exposes_doctor_mount_runtime_flag

echo "v0.16.4 mount runtime preflight UX validation passed"
