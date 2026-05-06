#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.17.5-cli-entrypoint-maintainability.md
require_pattern 'Phase 108: v0.17.5 CLI Entrypoint Maintainability Cleanup' docs/plan/development-phases.md
require_pattern 'Status: Completed' docs/plan/v0.17.5-cli-entrypoint-maintainability.md

require_file crates/operon-cli/src/cli_args.rs
require_file crates/operon-cli/src/cli_dispatch.rs
require_pattern 'mod cli_args;' crates/operon-cli/src/main.rs
require_pattern 'mod cli_dispatch;' crates/operon-cli/src/main.rs
require_pattern 'pub\(crate\) struct Args' crates/operon-cli/src/cli_args.rs
require_pattern 'pub\(crate\) enum Command' crates/operon-cli/src/cli_args.rs
require_pattern 'pub\(crate\) async fn dispatch' crates/operon-cli/src/cli_dispatch.rs
require_pattern 'cli_dispatch::dispatch\(cli_args::Args::parse\(\)\)' crates/operon-cli/src/main.rs

if rg -q 'enum Command|enum ExecCommand|match args.command|fn completion' crates/operon-cli/src/main.rs; then
  echo "main.rs should remain a thin parse-and-dispatch entrypoint after v0.17.5" >&2
  exit 1
fi

cargo check -p operon-cli --locked
cargo test -p operon-cli --locked clap_model_exposes_completion_command
cargo test -p operon-cli --locked --test cli_static_integration

echo "v0.17.5 CLI entrypoint maintainability validation passed"
