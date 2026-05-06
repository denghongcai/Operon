#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.18.1-mount-adapter-semantics-hardening.md
require_pattern 'Phase 110: v0.18.1 Mount Adapter Semantics Hardening' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.18.1-mount-adapter-semantics-hardening.md
require_pattern 'No v0.18.1 mount adapter semantics hardening work remains' docs/plan/development-phases.md

require_file crates/operon-mount/src/fuse_semantics.rs
require_pattern 'mod fuse_semantics;' crates/operon-mount/src/lib.rs
require_pattern 'fn rename_flags_errno' crates/operon-mount/src/fuse_semantics.rs
require_pattern 'fn xattr_decision' crates/operon-mount/src/fuse_semantics.rs
require_pattern 'enum XattrDecision' crates/operon-mount/src/fuse_semantics.rs
require_pattern 'rename_flags_are_rejected_explicitly' crates/operon-mount/src/fuse_semantics.rs
require_pattern 'xattr_semantics_preserve_missing_inode_and_empty_list_behavior' crates/operon-mount/src/fuse_semantics.rs
require_pattern 'rename_flags_errno\(flags\)' crates/operon-mount/src/fuse_fs.rs
require_pattern 'xattr_decision' crates/operon-mount/src/fuse_fs.rs
require_pattern 'fn refresh_inode_stat' crates/operon-mount/src/fuse_fs.rs
require_pattern 'refresh_inode_stat_updates_cached_write_and_truncate_metadata' crates/operon-mount/src/fuse_fs.rs
require_pattern 'remove_then_rename_replaces_destination_inode' crates/operon-mount/src/inode_table.rs
require_pattern 'rejects_invalid_child_segments' crates/operon-mount/src/mount_core.rs

cargo test -p operon-mount --locked fuse_semantics
cargo test -p operon-mount --locked remove_then_rename_replaces_destination_inode
cargo test -p operon-mount --locked refresh_inode_stat_updates_cached_write_and_truncate_metadata
cargo test -p operon-mount --locked rejects_invalid_child_segments
cargo check -p operon-mount --locked
if command -v rustup >/dev/null 2>&1; then
  rustup target add x86_64-pc-windows-gnu >/dev/null
fi
cargo check -p operon-mount --target x86_64-pc-windows-gnu --tests --locked

echo "v0.18.1 mount adapter semantics hardening validation passed"
