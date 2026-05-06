#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.17.3-mount-adapter-maintainability.md
require_pattern 'Phase 106: v0.17.3 Mount Adapter Maintainability Cleanup' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.17.3-mount-adapter-maintainability.md

require_file crates/operon-mount/src/fuse_attr.rs
require_pattern 'mod fuse_attr;' crates/operon-mount/src/lib.rs
require_pattern 'pub\(crate\) fn file_attr' crates/operon-mount/src/fuse_attr.rs
require_pattern 'pub\(crate\) fn attr_trace_detail' crates/operon-mount/src/fuse_attr.rs
require_pattern 'fuse_attr::\{attr_trace_detail, file_attr\}' crates/operon-mount/src/fuse_fs.rs
if rg -q 'fn file_attr|STAT_BLOCK_SIZE|fn attr_owner' crates/operon-mount/src/fuse_fs.rs; then
  echo "fuse_fs.rs should not own FUSE attribute mapping helpers after v0.17.3" >&2
  exit 1
fi

require_file crates/operon-mount/src/windows_security.rs
require_pattern 'mod windows_security;' crates/operon-mount/src/lib.rs
require_pattern 'WindowsSecurityDescriptor' crates/operon-mount/src/windows_security.rs
require_pattern 'windows_security::WindowsSecurityDescriptor' crates/operon-mount/src/windows.rs
if rg -q 'ConvertStringSecurityDescriptorToSecurityDescriptorW|GetSecurityDescriptorLength|LocalFree' crates/operon-mount/src/windows.rs; then
  echo "windows.rs should not own Windows security descriptor construction after v0.17.3" >&2
  exit 1
fi

cargo check -p operon-mount --locked
cargo test -p operon-mount --locked
if command -v rustup >/dev/null 2>&1; then
  rustup target add x86_64-pc-windows-gnu >/dev/null
fi
cargo check -p operon-mount --target x86_64-pc-windows-gnu --tests --locked

echo "v0.17.3 mount adapter maintainability validation passed"
