#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.9.3-store-backed-audit-visibility.md
require_pattern 'load_audit_events' crates/operon-store/src/lib.rs
require_pattern 'load_audit_events\(store.as_deref\(\)\)' crates/operond/src/daemon_state.rs
require_pattern 'bounded_audit_events\(stored_audit_events\)' crates/operond/src/daemon_state.rs
require_pattern 'bounded_audit_events_keeps_recent_persisted_events' crates/operond/src/audit.rs
require_pattern 'load_audit_events_reads_persisted_audit_records_in_order' crates/operon-store/src/lib.rs
require_pattern 'v0.9.3 Store-Backed Audit Visibility' docs/plan/development-phases.md
require_pattern 'scripts/verify-v0.9.3-store-backed-audit-visibility.sh' DEVELOPMENT.md
require_pattern 'scripts/verify-v0.9.3-store-backed-audit-visibility.sh' scripts/ci/run-validations.sh
require_pattern 'store-backed audit validation' DEVELOPMENT.md

cargo test -p operon-store --locked load_audit_events_reads_persisted_audit_records_in_order
cargo test -p operond --locked bounded_audit_events_keeps_recent_persisted_events
bash scripts/verify-docs-help-skills-sync.sh

echo "v0.9.3 store-backed audit visibility validation passed"
