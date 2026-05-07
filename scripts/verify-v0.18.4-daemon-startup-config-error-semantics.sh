#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.18.4-daemon-startup-config-error-semantics.md
require_pattern 'Phase 113: v0.18.4 Daemon Startup / Config Error Semantics' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.18.4-daemon-startup-config-error-semantics.md
require_pattern 'No v0.18.4 daemon startup/config error semantics work remains' docs/plan/development-phases.md

require_pattern 'enum DaemonStartupErrorKind' crates/operond/src/daemon_state.rs
require_pattern 'struct DaemonStartupError' crates/operond/src/daemon_state.rs
require_pattern 'ConfigLoad => "config-load"' crates/operond/src/daemon_state.rs
require_pattern 'ConfigParse => "config-parse"' crates/operond/src/daemon_state.rs
require_pattern 'DaemonSection => "daemon-section"' crates/operond/src/daemon_state.rs
require_pattern 'AuthToken => "auth-token"' crates/operond/src/daemon_state.rs
require_pattern 'StoreConfig => "store-config"' crates/operond/src/daemon_state.rs
require_pattern 'StateRestore => "state-restore"' crates/operond/src/daemon_state.rs
require_pattern 'Secrets => "secrets"' crates/operond/src/daemon_state.rs
require_pattern 'ServerBind => "server-bind"' crates/operond/src/daemon_state.rs
require_pattern 'from_str_with_warnings' crates/operond/src/daemon_state.rs
require_pattern 'server_start_error' crates/operond/src/main.rs crates/operond/src/daemon_state.rs
require_pattern 'daemon_state_startup_errors_classify_missing_config_file' crates/operond/src/daemon_state.rs
require_pattern 'daemon_state_startup_errors_classify_auth_token_resolution' crates/operond/src/daemon_state.rs
require_pattern 'daemon_state_startup_errors_classify_store_config' crates/operond/src/daemon_state.rs
require_pattern 'daemon_state_startup_errors_classify_state_restore' crates/operond/src/daemon_state.rs
require_pattern 'daemon_state_startup_errors_classify_secrets' crates/operond/src/daemon_state.rs
require_pattern 'daemon_state_startup_errors_classify_server_bind' crates/operond/src/daemon_state.rs

cargo test -p operond --locked daemon_state_startup_errors

echo "v0.18.4 daemon startup/config error semantics validation passed"
