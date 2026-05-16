#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file AGENTS.md
require_file README.md
require_file PROTOCOL.md
require_file DEVELOPMENT.md
require_file docs/architecture/runtime-api.md
require_file docs/architecture/technology-and-protocol-decisions.md
require_file docs/plan/development-phases.md
require_file docs/plan/v0.10-exec-unification.md
require_file proto/operon/runtime.proto
require_file crates/operon-core/src/exec.rs
require_file crates/operond/src/exec_runtime.rs
require_file crates/operon-cli/src/commands/exec.rs
require_file skills/operon-fs-exec/SKILL.md

require_pattern 'v0.10 Execution Capability Unification' docs/plan/development-phases.md
require_pattern 'Execution Capability Unification' docs/plan/v0.10-exec-unification.md
require_pattern 'exec.session' docs/plan/v0.10-exec-unification.md
require_pattern 'PTY/TTY' docs/plan/v0.10-exec-unification.md
require_pattern 'scripts/verify-v0.10-exec-unification.sh' DEVELOPMENT.md
require_pattern 'scripts/verify-v0.10-exec-unification.sh' scripts/ci/run-validations.sh
require_pattern 'PROTOCOL_VERSION: &str = "v0.16.7"' crates/operon-protocol/src/lib.rs
require_pattern '"version": "0.16.7"' packages/sdk-js/package.json

require_pattern 'rpc RunExec' proto/operon/runtime.proto
require_pattern 'rpc GetExec' proto/operon/runtime.proto
require_pattern 'rpc ListExecs' proto/operon/runtime.proto
require_pattern 'rpc WatchExec' proto/operon/runtime.proto
require_pattern 'rpc StreamExecLogs' proto/operon/runtime.proto
require_pattern 'CAPABILITY_KIND_EXEC' proto/operon/runtime.proto
reject_pattern 'CAPABILITY_KIND_J[o]B' proto/operon/runtime.proto
reject_pattern 'rpc .*J[o]b' proto/operon/runtime.proto
reject_pattern 'message .*J[o]b' proto/operon/runtime.proto

help_commands=(
  "exec --help"
  "exec run --help"
  "exec list --help"
  "exec status --help"
  "exec logs --help"
  "exec stdin --help"
  "exec cancel --help"
)

for command in "${help_commands[@]}"; do
  if ! output="$(cargo run -q -p operon-cli -- $command 2>&1)"; then
    echo "help command failed: operon $command" >&2
    echo "$output" >&2
    exit 1
  fi
  if ! grep -Eq 'Usage:|Commands:|Options:' <<<"$output"; then
    echo "help command did not look like clap help: operon $command" >&2
    echo "$output" >&2
    exit 1
  fi
done

if cargo run -q -p operon-cli -- job --help >/tmp/operon-v0.10-job-help.out 2>&1; then
  echo "operon job should not remain a supported active command" >&2
  cat /tmp/operon-v0.10-job-help.out >&2
  exit 1
fi

active_docs=(
  AGENTS.md
  README.md
  PROTOCOL.md
  DEVELOPMENT.md
  docs/architecture/runtime-api.md
  docs/architecture/technology-and-protocol-decisions.md
  skills/operon-cli-ops/SKILL.md
  skills/operon-core/SKILL.md
  skills/operon-fs-exec/SKILL.md
  skills/operon-sdk-protocol/SKILL.md
)

for path in "${active_docs[@]}"; do
  reject_pattern '\boperon j[o]b\b' "$path"
  reject_pattern '\bj[o]b\.run\b' "$path"
  reject_pattern '\bj[o]b:default\b' "$path"
  reject_pattern '\bpolicy\.j[o]b\b' "$path"
  reject_pattern '\bJ[o]b[A-Z][A-Za-z]*\b' "$path"
done

require_pattern '\boperon exec run\b' README.md
require_pattern '\bexec\.run\b' README.md
require_pattern '\bexec:default\b' PROTOCOL.md
require_pattern '\bpolicy\.exec\b' DEVELOPMENT.md
require_pattern '\bexec\.run\b' skills/operon-fs-exec/SKILL.md

reject_pattern '\bJ[o]bRunRequest\b' crates packages proto
reject_pattern '\bJ[o]bRecord\b' crates packages proto
reject_pattern '\brunJ[o]b\b' packages/sdk-js/src
require_pattern '\bExecRunRequest\b' crates/operon-core/src/exec.rs
require_pattern '\brunExec\b' packages/sdk-js/src/index.ts

echo "v0.10 exec unification validation passed"
