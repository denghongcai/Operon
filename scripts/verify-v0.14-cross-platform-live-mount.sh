#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.14-cross-platform-live-mount.md
require_file docs/plan/v0.14-macos-live-smoke-runbook.md
require_file .github/workflows/v0.14-live-mount-smoke.yml
require_file scripts/preflight-v0.14-macos-fuse-t-host.sh
require_file scripts/install-v0.14-macos-fuse-t.sh
require_file scripts/verify-v0.14-release-gates.sh
require_file scripts/smoke-v0.14-macos-live-mount.sh
require_file scripts/smoke-v0.14-macos-fuse-zip-probe.sh
require_file scripts/smoke-v0.14-macos-libfuse-lowlevel-hello-probe.sh
require_file scripts/smoke-v0.14-windows-live-mount.ps1
require_file vendor/fuser-0.17.0-operon/OPERON_PATCH.md
require_file vendor/fuser-0.17.0-operon/src/lib.rs
require_file vendor/fuser-0.17.0-operon/src/ll/request.rs
require_pattern 'Phase 93: v0.14 Cross-Platform Live Mount' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.14-cross-platform-live-mount.md
require_pattern 'FUSE-T replaces macFUSE as the active macOS live-smoke runtime' docs/plan/v0.14-cross-platform-live-mount.md
require_pattern 'v0.14 macOS live mount smoke passed' docs/plan/v0.14-macos-live-smoke-runbook.md
require_pattern 'macos_runner=hosted' docs/plan/v0.14-macos-live-smoke-runbook.md
require_pattern 'macos_backend=nfs' docs/plan/v0.14-macos-live-smoke-runbook.md
require_pattern 'OPERON_MOUNT_MACOS_OPTIONS=nobrowse,noattrcache' docs/plan/v0.14-macos-live-smoke-runbook.md
require_pattern 'scripts/preflight-v0.14-macos-fuse-t-host.sh' docs/plan/v0.14-macos-live-smoke-runbook.md
require_pattern 'docs/plan/v0.14-cross-platform-live-mount.md' AGENTS.md
require_pattern 'docs/plan/v0.14-macos-live-smoke-runbook.md' AGENTS.md
require_pattern 'v0.14 Cross-Platform Live Mount' AGENTS.md
require_pattern 'Linux FUSE, macOS FUSE-T, and Windows WinFsp' README.md
require_pattern 'Linux uses FUSE, macOS uses FUSE-T, and Windows uses WinFsp' PROTOCOL.md
require_pattern 'native Windows WinFsp adapter' AGENTS.md
require_pattern 'MIT `winfsp_wrs` / `winfsp_wrs_sys`' docs/architecture/technology-and-protocol-decisions.md
require_pattern 'fuser = \{ path = "vendor/fuser-0\.17\.0-operon" \}' Cargo.toml
require_pattern 'const INIT_FLAGS: InitFlags = InitFlags::FUSE_ASYNC_READ;' vendor/fuser-0.17.0-operon/src/lib.rs
require_pattern '#\[cfg\(target_os = "macos"\)\]' vendor/fuser-0.17.0-operon/src/ll/request.rs
require_pattern 'let flags = config.requested;' vendor/fuser-0.17.0-operon/src/ll/request.rs
require_pattern 'pub\(crate\) fn fuse_mount\(mountpoint: \*const c_char, args: \*const fuse_args\) -> \*mut c_void;' vendor/fuser-0.17.0-operon/src/mnt/fuse2_sys.rs
require_pattern 'let channel = unsafe \{ fuse_mount\(mountpoint.as_ptr\(\), args\) \};' vendor/fuser-0.17.0-operon/src/mnt/fuse2.rs
require_pattern 'let fd = unsafe \{ fuse_chan_fd\(channel\) \};' vendor/fuser-0.17.0-operon/src/mnt/fuse2.rs

require_pattern 'pub struct MountAdapterCore' crates/operon-mount/src/mount_core.rs
require_pattern 'pub struct MountDirectoryEntry' crates/operon-mount/src/mount_core.rs
require_pattern 'pub enum MountErrorKind' crates/operon-mount/src/mount_core.rs
require_pattern 'pub fn classify_mount_error' crates/operon-mount/src/mount_core.rs
require_pattern 'MountAdapterCore::new' crates/operon-mount/src/fuse_fs.rs
require_pattern 'fn getxattr' crates/operon-mount/src/fuse_fs.rs
require_pattern 'fn listxattr' crates/operon-mount/src/fuse_fs.rs
require_pattern 'Errno::NO_XATTR' crates/operon-mount/src/fuse_fs.rs
require_pattern '#!\[cfg\(any\(target_os = "linux", target_os = "macos"\)\)\]' crates/operon-mount/src/fuse_fs.rs
require_pattern '#\[cfg\(any\(target_os = "linux", target_os = "macos"\)\)\]' crates/operon-mount/src/lib.rs
require_pattern 'macos-no-mount = \["fuser/macos-no-mount"\]' crates/operon-mount/Cargo.toml
require_pattern 'fn base_mount_options\(\) -> Vec<fuser::MountOption>' crates/operon-mount/src/session.rs
require_pattern 'fn default_mount_thread_count\(\) -> usize' crates/operon-mount/src/session.rs
require_pattern 'trace_mount_event\("n_threads"' crates/operon-mount/src/session.rs
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
require_pattern 'macos-fuse-t' crates/operon-cli/src/commands/mount.rs
require_pattern 'windows-winfsp' crates/operon-cli/src/commands/mount.rs
require_pattern 'macos-fuse-t-supported-runtime-required' crates/operon-cli/src/commands/doctor.rs
require_pattern 'windows-winfsp-supported-runtime-required' crates/operon-cli/src/commands/doctor.rs
require_pattern 'PROTOCOL_VERSION: &str = "v0\.14\.0"' crates/operon-protocol/src/lib.rs
require_pattern '"version": "0\.14\.0"' packages/sdk-js/package.json
require_pattern 'cargo test -p operon-mount --locked --features macos-no-mount' .github/workflows/ci.yml
require_pattern 'cargo test -p operon-mount --locked' .github/workflows/ci.yml
require_pattern 'actions: read' .github/workflows/release-draft.yml
require_pattern 'scripts/install-v0.14-macos-fuse-t.sh' .github/workflows/release-draft.yml
require_pattern 'v014-release-gate' .github/workflows/release-draft.yml
require_pattern 'scripts/verify-v0.14-release-gates.sh "\$GITHUB_REF_NAME" "\$GITHUB_SHA" "\$GITHUB_REPOSITORY"' .github/workflows/release-draft.yml
require_pattern 'macOS FUSE-T Live Mount \(hosted\)' scripts/verify-v0.14-release-gates.sh
require_pattern 'Windows WinFsp Live Mount' scripts/verify-v0.14-release-gates.sh
require_pattern 'missing release gate' scripts/verify-v0.14-release-gates.sh
require_pattern '\$gate_name live mount release gate passed' scripts/verify-v0.14-release-gates.sh
require_pattern 'scripts/install-v0.14-macos-fuse-t.sh' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'brew install macos-fuse-t/homebrew-cask/fuse-t' scripts/install-v0.14-macos-fuse-t.sh
require_pattern 'macos_backend:' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'macos_runner:' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'macos_options:' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'fuser_patch_init_flags:' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'OPERON_FUSER_HELLO_PATCH_INIT_FLAGS: \$\{\{ inputs\.fuser_patch_init_flags && '\''1'\'' \|\| '\''0'\'' \}\}' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'macos-libfuse-lowlevel-hello' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'macOS FUSE-T libfuse Low-Level Hello Probe \(hosted\)' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'scripts/smoke-v0.14-macos-libfuse-lowlevel-hello-probe.sh' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'macos-fuse-zip' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'macOS FUSE-T fuse-zip Probe \(hosted\)' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'scripts/smoke-v0.14-macos-fuse-zip-probe.sh' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'self-hosted-fuse-t' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'runs-on: \[self-hosted, macOS, fuse-t\]' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'Check FUSE-T runtime' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'scripts/preflight-v0.14-macos-fuse-t-host.sh' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'OPERON_MOUNT_MACOS_BACKEND: \$\{\{ inputs.macos_backend \}\}' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'OPERON_MOUNT_MACOS_OPTIONS: \$\{\{ inputs.macos_options \}\}' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'macOS live mount smoke exit code' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'actions/upload-artifact@v7' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'choco install winfsp -y' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'scripts/smoke-v0.14-macos-live-mount.sh' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'scripts/smoke-v0.14-windows-live-mount.ps1' .github/workflows/v0.14-live-mount-smoke.yml
require_pattern 'SMOKE_TIMEOUT_SECS="\$\{OPERON_SMOKE_TIMEOUT_SECS:-600\}"' scripts/smoke-v0.14-macos-live-mount.sh
require_pattern 'wait_for_process_exit' scripts/smoke-v0.14-macos-live-mount.sh
require_pattern 'macOS mount backend: \$OPERON_MOUNT_MACOS_BACKEND' scripts/smoke-v0.14-macos-live-mount.sh
require_pattern 'macOS mount extra options: \$\{OPERON_MOUNT_MACOS_OPTIONS:-<none>\}' scripts/smoke-v0.14-macos-live-mount.sh
require_pattern 'perl -e .*macOS live mount smoke timed out' scripts/smoke-v0.14-macos-live-mount.sh
require_pattern 'Library/Logs/fuse-t' scripts/smoke-v0.14-macos-live-mount.sh
require_pattern 'v0\.14 macOS FUSE-T fuse-zip probe passed' scripts/smoke-v0.14-macos-fuse-zip-probe.sh
require_pattern 'https://github.com/macos-fuse-t/fuse-zip' scripts/smoke-v0.14-macos-fuse-zip-probe.sh
require_pattern 'v0\.14 macOS FUSE-T host preflight passed' scripts/preflight-v0.14-macos-fuse-t-host.sh

bash -n scripts/preflight-v0.14-macos-fuse-t-host.sh
bash -n scripts/install-v0.14-macos-fuse-t.sh
bash -n scripts/verify-v0.14-release-gates.sh
bash -n scripts/smoke-v0.14-macos-live-mount.sh
bash -n scripts/smoke-v0.14-macos-fuse-zip-probe.sh
bash -n scripts/smoke-v0.14-macos-libfuse-lowlevel-hello-probe.sh

scripts/verify-v0.14-release-gates.sh test-tag HEAD >/dev/null

rustup target add x86_64-apple-darwin x86_64-pc-windows-gnu >/dev/null

cargo test -p operon-mount --locked --lib
cargo check -p operon-mount --target x86_64-apple-darwin --locked --features macos-no-mount
cargo check -p operon-cli --target x86_64-apple-darwin --locked --features operon-mount/macos-no-mount
cargo check -p operon-mount --target x86_64-pc-windows-gnu --locked
cargo check -p operon-cli --target x86_64-pc-windows-gnu --locked

echo "v0.14 cross-platform live mount validation passed"
