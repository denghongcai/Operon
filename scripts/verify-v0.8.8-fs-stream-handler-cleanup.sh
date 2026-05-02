#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.8.8-fs-stream-handler-cleanup.md
require_file crates/operond/src/fs_service.rs
require_pattern 'pub\(crate\) type FileStream' crates/operond/src/fs_service.rs
require_pattern 'pub\(crate\) async fn read_stream' crates/operond/src/fs_service.rs
require_pattern 'pub\(crate\) async fn write_stream' crates/operond/src/fs_service.rs
require_pattern 'fs_service::read_stream' crates/operond/src/main.rs
require_pattern 'fs_service::write_stream' crates/operond/src/main.rs
reject_pattern 'read-stream' crates/operond/src/main.rs
reject_pattern 'write stream target metadata' crates/operond/src/main.rs
reject_pattern 'write stream chunk arrived' crates/operond/src/main.rs
require_pattern 'v0.8.8 Filesystem Stream Handler Cleanup' docs/plan/development-phases.md

cargo test -p operond --locked

echo "v0.8.8 filesystem stream handler cleanup validation passed"
