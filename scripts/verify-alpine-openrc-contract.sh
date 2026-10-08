#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
source scripts/lib/validation.sh
require_file docs/quality/alpine-openrc.md
require_file crates/operond/src/service_linux.rs
require_file crates/operond/src/service_openrc.rs
require_pattern 'Native rebooted Alpine OpenRC service acceptance' .github/workflows/alpine-musl-acceptance.yml
require_pattern 'scripts/run-alpine-openrc-acceptance.sh' .github/workflows/alpine-musl-acceptance.yml
require_pattern 'shutdown-timeout-secs' docs/quality/alpine-openrc.md
bash -n scripts/run-alpine-openrc-acceptance.sh
python3 -c 'import ast; ast.parse(open("scripts/alpine-openrc-acceptance.py").read())'
# These are contract tests, not a substitute for the required rebooted native
# Alpine lifecycle job on both architectures and both pinned releases.
cargo test -p operond --locked service_
cargo test -p operond --locked shutdown_cancels_running_exec_and_refuses_new_registration
echo 'Alpine/OpenRC source and shutdown contracts passed; real native lifecycle is a separate required gate'
