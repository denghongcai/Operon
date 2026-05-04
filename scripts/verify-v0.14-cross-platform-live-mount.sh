#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.14-cross-platform-live-mount.md
require_file .github/workflows/v0.14-live-mount-smoke.yml
require_file scripts/smoke-v0.14-macos-live-mount.sh
require_file scripts/smoke-v0.14-windows-live-mount.ps1
require_pattern 'Phase 93: v0.14 Cross-Platform Live Mount' docs/plan/development-phases.md
require_pattern 'Status: In progress' docs/plan/v0.14-cross-platform-live-mount.md
require_pattern 'macOS live smoke on a host with macFUSE installed remains' docs/plan/v0.14-cross-platform-live-mount.md
require_pattern 'docs/plan/v0.14-cross-platform-live-mount.md' AGENTS.md
require_pattern 'v0.14 Cross-Platform Live Mount' AGENTS.md
require_pattern 'Linux FUSE, macOS macFUSE, and Windows WinFsp' README.md
require_pattern 'Linux uses FUSE, macOS uses macFUSE, and Windows uses WinFsp' PROTOCOL.md
require_pattern 'native Windows WinFsp adapter' AGENTS.md
require_pattern 'MIT `winfsp_wrs` / `winfsp_wrs_sys`' docs/architecture/technology-and-protocol-decisions.md

require_pattern 'pub struct MountAdapterCore' crates/operon-mount/src/mount_core.rs
require_pattern 'pub struct MountDirectoryEntry' crates/operon-mount/src/mount_core.rs
require_pattern 'pub enum MountErrorKind' crates/operon-mount/src/mount_core.rs
require_pattern 'pub fn classify_mount_error' crates/operon-mount/src/mount_core.rs
require_pattern 'MountAdapterCore::new' crates/operon-mount/src/fuse_fs.rs
require_pattern '#!\[cfg\(any\(target_os = "linux", target_os = "macos"\)\)\]' crates/operon-mount/src/fuse_fs.rs
require_pattern '#\[cfg\(any\(target_os = "linux", target_os = "macos"\)\)\]' crates/operon-mount/src/lib.rs
require_pattern 'macos-no-mount = \["fuser/macos-no-mount"\]' crates/operon-mount/Cargo.toml
require_pattern 'winfsp_wrs = "0\.4\.1"' crates/operon-mount/Cargo.toml
require_pattern 'winfsp_wrs_sys = "0\.4\.1"' crates/operon-mount/Cargo.toml
require_pattern 'mod windows;' crates/operon-mount/src/lib.rs
require_file crates/operon-mount/src/windows.rs
require_pattern 'FSP_FILE_SYSTEM_INTERFACE' crates/operon-mount/src/windows.rs
require_pattern 'FspFileSystemCreate' crates/operon-mount/src/windows.rs
require_pattern 'CreateEx: Some\(create_ex_cb\)' crates/operon-mount/src/windows.rs
require_pattern 'Overwrite: Some\(overwrite_cb\)' crates/operon-mount/src/windows.rs
require_pattern 'windows_name_to_remote_path' crates/operon-mount/src/windows.rs
require_pattern 'write_to_eof' crates/operon-mount/src/windows.rs
require_pattern 'cfg\(any\(target_os = "linux", target_os = "macos"\)\)' crates/operon-cli/Cargo.toml
require_pattern 'operon-mount = \{ path = "../operon-mount" \}' crates/operon-cli/Cargo.toml
require_pattern 'macos-macfuse' crates/operon-cli/src/commands/mount.rs
require_pattern 'windows-winfsp' crates/operon-cli/src/commands/mount.rs
require_pattern 'macos-macfuse-supported-runtime-required' crates/operon-cli/src/commands/doctor.rs
require_pattern 'windows-winfsp-supported-runtime-required' crates/operon-cli/src/commands/doctor.rs
require_pattern 'PROTOCOL_VERSION: &str = "v0\.14\.0"' crates/operon-protocol/src/lib.rs
require_pattern '"version": "0\.14\.0"' packages/sdk-js/package.json
require_pattern 'cargo test -p operon-mount --locked --features macos-no-mount' .github/workflows/ci.yml
require_pattern 'cargo test -p operon-mount --locked' .github/workflows/ci.yml
require_pattern 'brew install --cask macfuse' .github/workflows/release-draft.yml
require_pattern 'brew install --cask macfuse' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'macos_backend:' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'OPERON_MOUNT_MACOS_BACKEND: \$\{\{ inputs.macos_backend \}\}' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'macOS live mount smoke exit code' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'actions/upload-artifact@v7' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'choco install winfsp -y' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'scripts/smoke-v0.14-macos-live-mount.sh' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'scripts/smoke-v0.14-windows-live-mount.ps1' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'SMOKE_TIMEOUT_SECS="\$\{OPERON_SMOKE_TIMEOUT_SECS:-600\}"' scripts/smoke-v0.14-macos-live-mount.sh
require_pattern 'wait_for_process_exit' scripts/smoke-v0.14-macos-live-mount.sh

bash -n scripts/smoke-v0.14-macos-live-mount.sh

rustup target add x86_64-apple-darwin x86_64-pc-windows-gnu >/dev/null

cargo test -p operon-mount --locked --lib
cargo check -p operon-mount --target x86_64-apple-darwin --locked --features macos-no-mount
cargo check -p operon-cli --target x86_64-apple-darwin --locked --features operon-mount/macos-no-mount
cargo check -p operon-mount --target x86_64-pc-windows-gnu --locked
cargo check -p operon-cli --target x86_64-pc-windows-gnu --locked

echo "v0.14 cross-platform live mount validation passed"
