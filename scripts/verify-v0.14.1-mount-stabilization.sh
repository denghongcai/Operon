#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.14.1-mount-stabilization.md
require_pattern 'Phase 94: v0.14.1 Mount Stabilization' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.14.1-mount-stabilization.md
require_pattern 'No v0.14.1 mount stabilization work remains' docs/plan/v0.14.1-mount-stabilization.md

require_pattern 'AlreadyExists' crates/operon-mount/src/mount_core.rs
require_pattern 'tonic::Code::AlreadyExists' crates/operon-mount/src/mount_core.rs
require_pattern 'MountErrorKind::AlreadyExists => fuser::Errno::EEXIST' crates/operon-mount/src/errors.rs
require_pattern 'MountErrorKind::AlreadyExists => STATUS_OBJECT_NAME_COLLISION' crates/operon-mount/src/windows_status.rs
require_pattern 'tonic::Status::already_exists' crates/operon-mount/src/mount_core.rs crates/operon-mount/src/errors.rs

cargo test -p operon-mount --locked classifies_remote_errors_without_platform_errno
cargo test -p operon-mount --locked maps_tonic_statuses_to_fuse_errno

echo "v0.14.1 mount stabilization validation passed"
