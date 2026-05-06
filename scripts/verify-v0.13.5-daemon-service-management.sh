#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.13.5-daemon-service-management.md
require_pattern 'Status: Completed' docs/plan/v0.13.5-daemon-service-management.md
require_pattern 'Phase 89: v0.13.5 Daemon Service Management' docs/plan/development-phases.md
require_pattern 'No v0.13.5 daemon service management work remains' docs/plan/development-phases.md

require_file crates/operond/src/service_manager.rs
require_pattern 'enum ServiceCommand' crates/operond/src/daemon_cli.rs
require_pattern 'ServiceCommand::Install' crates/operond/src/main.rs
require_pattern 'service_manager::install' crates/operond/src/main.rs
require_pattern 'render_systemd_user_unit' crates/operond/src/service_manager.rs
require_pattern 'render_launchd_user_plist' crates/operond/src/service_manager.rs
require_pattern 'windows_service_create_args' crates/operond/src/service_manager.rs
require_pattern 'StartServiceCtrlDispatcherW' crates/operond/src/service_manager.rs
require_pattern 'RegisterServiceCtrlHandlerExW' crates/operond/src/service_manager.rs
require_pattern 'SetServiceStatus' crates/operond/src/service_manager.rs
require_pattern 'systemctl' crates/operond/src/service_manager.rs
require_pattern 'launchctl' crates/operond/src/service_manager.rs
require_pattern 'Win32_System_Services' crates/operond/Cargo.toml
require_pattern 'cargo test -p operond --locked service_management' .github/workflows/ci.yml
require_pattern 'operond service --help' scripts/verify-readme-quickstart-docker.sh
require_pattern 'operond start --config <path>' DEVELOPMENT.md docs/plan/v0.13.5-daemon-service-management.md
require_pattern 'Windows' README.md DEVELOPMENT.md AGENTS.md docs/plan/v0.13.5-daemon-service-management.md
require_pattern 'operond service run --config <path>' DEVELOPMENT.md AGENTS.md docs/plan/v0.13.5-daemon-service-management.md
reject_pattern 'operond start --background' crates/operond/src README.md DEVELOPMENT.md
reject_pattern 'degraded' README.md DEVELOPMENT.md docs/plan/v0.13.5-daemon-service-management.md

for command in \
  "--help" \
  "start --help" \
  "service --help" \
  "service install --help" \
  "service start --help" \
  "service stop --help" \
  "service status --help" \
  "service uninstall --help"
do
  if ! output="$(cargo run -q -p operond -- $command 2>&1)"; then
    echo "help command failed: operond $command" >&2
    echo "$output" >&2
    exit 1
  fi
  if ! grep -Eq 'Usage:|Commands:|Options:' <<<"$output"; then
    echo "help command did not look like clap help: operond $command" >&2
    echo "$output" >&2
    exit 1
  fi
done

start_help="$(cargo run -q -p operond -- start --help 2>&1)"
if grep -q -- '--background' <<<"$start_help"; then
  echo "operond start help unexpectedly exposes --background" >&2
  echo "$start_help" >&2
  exit 1
fi

cargo test -p operond --locked service_management

if ! rustup target list --installed | rg '^x86_64-pc-windows-gnu$' >/dev/null; then
  rustup target add x86_64-pc-windows-gnu
fi
cargo check -p operond --locked --target x86_64-pc-windows-gnu

echo "v0.13.5 daemon service management validation passed"
