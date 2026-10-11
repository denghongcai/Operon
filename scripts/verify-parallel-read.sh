#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
cargo test --locked -p operon-config mount_read_configuration
cargo test --locked -p operon-cli mount_read_overrides
cargo test --locked -p operon-mount read_queue_deadline
cargo test --locked -p operon-core mount_read_tests
cargo test --locked -p operon-mount read_scheduler::tests
cargo test --locked -p operon-mount shutdown_cancels_deadline_disabled_reads
cargo test --locked -p operon-mount mount_identity_handles_detachment_replacement
cargo test --locked -p operond range_fill
python3 scripts/performance/parallel-read.py --help >/dev/null
python3 scripts/performance/async-read-probes.py --help >/dev/null
echo "Parallel read configuration, bounded scheduling, interrupt/shutdown cancellation and range tests passed"
