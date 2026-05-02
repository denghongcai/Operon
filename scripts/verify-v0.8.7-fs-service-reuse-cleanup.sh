#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.8.7-fs-service-reuse-cleanup.md
require_file crates/operond/src/fs_service.rs
require_pattern 'fn authorize_fs_action' crates/operond/src/fs_service.rs
require_pattern 'fn resolve_existing_path' crates/operond/src/fs_service.rs
require_pattern 'fn resolve_existing_leaf_path' crates/operond/src/fs_service.rs
require_pattern 'fn resolve_write_path' crates/operond/src/fs_service.rs
require_pattern 'fn resolve_create_path' crates/operond/src/fs_service.rs
reject_pattern 'authorize_fs\(&state.policy, "[a-z-]+"' crates/operond/src/fs_service.rs
require_pattern 'v0.8.7 Filesystem Service Reuse Cleanup' docs/plan/development-phases.md

cargo test -p operond --locked

echo "v0.8.7 filesystem service reuse cleanup validation passed"
