#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.13.8-mount-core-boundary.md
require_pattern 'Status: Completed' docs/plan/v0.13.8-mount-core-boundary.md
require_pattern 'Phase 92: v0.13.8 Mount Core Boundary' docs/plan/development-phases.md
require_pattern 'No v0.13.8 mount-core boundary work remains' docs/plan/development-phases.md

require_file crates/operon-mount/src/mount_core.rs
require_pattern 'pub mod mount_core;' crates/operon-mount/src/lib.rs
require_pattern 'pub use mount_core::RemoteFs' crates/operon-mount/src/lib.rs
reject_pattern '^#!\[cfg\(target_os = "linux"\)\]' crates/operon-mount/src/lib.rs
require_pattern '#\[cfg\(target_os = "linux"\)\]' crates/operon-mount/src/lib.rs
require_pattern 'pub trait RemoteFs' crates/operon-mount/src/mount_core.rs
require_pattern 'pub fn normalize_remote_path' crates/operon-mount/src/mount_core.rs
require_pattern 'pub fn validate_child_name' crates/operon-mount/src/mount_core.rs
require_pattern 'pub fn join_remote_child' crates/operon-mount/src/mount_core.rs
require_pattern 'use crate::mount_core::RemoteFs' crates/operon-mount/src/remote_client.rs
require_pattern 'mount_core::RemoteFs' crates/operon-mount/src/lib.rs
require_pattern 'mount_core_api_is_available_from_crate_root' crates/operon-mount/src/lib.rs

cargo test -p operon-mount --locked --lib

echo "v0.13.8 mount-core boundary validation passed"
