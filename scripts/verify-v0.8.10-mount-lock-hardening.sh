#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.8.10-mount-lock-hardening.md
require_file crates/operon-mount/src/fuse_fs.rs
require_pattern 'fn write_inodes' crates/operon-mount/src/fuse_fs.rs
require_pattern 'inode table poisoned' crates/operon-mount/src/fuse_fs.rs
reject_pattern 'expect\("inode table poisoned"\)' crates/operon-mount/src/fuse_fs.rs
reject_pattern '\.write\(\)\.expect' crates/operon-mount/src/fuse_fs.rs
require_pattern 'v0.8.10 Mount Lock Hardening' docs/plan/development-phases.md

cargo test -p operon-mount --locked

echo "v0.8.10 mount lock hardening validation passed"
