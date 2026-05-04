#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.11.3-platform-capability-matrix.md
require_pattern 'Status: Completed' docs/plan/v0.11.3-platform-capability-matrix.md
require_pattern 'Phase 77: v0.11.3 Platform Capability Matrix and CI Smoke' docs/plan/development-phases.md
require_pattern 'No v0.11.3 work remains' docs/plan/development-phases.md

require_pattern 'rust-platform-smoke' .github/workflows/ci.yml
require_pattern 'macos-latest' .github/workflows/ci.yml
require_pattern 'windows-latest' .github/workflows/ci.yml
require_pattern 'arduino/setup-protoc@v3' .github/workflows/ci.yml
require_pattern 'repo-token: \$\{\{ github\.token \}\}' .github/workflows/ci.yml
require_pattern 'cargo check --workspace --locked' .github/workflows/ci.yml
require_pattern 'cargo test -p operond --locked shell_invocation_matches_platform' .github/workflows/ci.yml
require_pattern 'cargo test -p operon-cli --locked exec_session_terminal_dimensions' .github/workflows/ci.yml

require_pattern 'Linux FUSE, macOS macFUSE, and Windows WinFsp' README.md
require_pattern 'macOS and Windows hosts' README.md
require_pattern 'must have the corresponding platform runtime installed' README.md
require_pattern 'supported through `portable-pty` on Unix-like platforms' docs/architecture/technology-and-protocol-decisions.md
require_pattern 'Windows interactive exec sessions are' PROTOCOL.md
require_pattern 'explicitly unsupported in this release line' PROTOCOL.md
require_pattern 'Mount adapter | Linux FUSE supported | Deferred macFUSE | Deferred WinFsp' docs/plan/v0.11.3-platform-capability-matrix.md

require_pattern 'fn exec_shell_program' crates/operond/src/exec_runtime.rs
require_pattern 'fn session_shell_program' crates/operond/src/exec_session.rs
require_pattern '"cmd.exe"' crates/operond/src/exec_runtime.rs
require_pattern '"cmd.exe"' crates/operond/src/exec_session.rs
require_pattern '"/bin/sh"' crates/operond/src/exec_runtime.rs
require_pattern '"/bin/sh"' crates/operond/src/exec_session.rs

cargo test -p operond --locked shell_invocation_matches_platform

echo "v0.11.3 platform capability matrix validation passed"
