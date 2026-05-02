#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file AGENTS.md
require_file README.md
require_file PROTOCOL.md
require_file docs/plan/development-phases.md

docs_and_skills=(
  AGENTS.md
  README.md
  PROTOCOL.md
)

while IFS= read -r path; do
  docs_and_skills+=("$path")
done < <(find docs skills -type f -name '*.md' | sort)

for path in "${docs_and_skills[@]}"; do
  reject_pattern 'operon node discover --provider' "$path"
  reject_pattern 'node discover --provider' "$path"
  reject_pattern 'operon provider\b' "$path"
  reject_pattern 'provider list\b' "$path"
  reject_pattern 'provider discovery' "$path"
  reject_pattern 'provider adapters' "$path"
  reject_pattern 'commands/provider' "$path"
done

require_pattern 'mDNS discovery is only a convenience mechanism' AGENTS.md
require_pattern 'Skills explain scenarios and command choice; CLI help is the source of truth' AGENTS.md
require_pattern 'scripts/verify-docs-help-skills-sync.sh' AGENTS.md
require_pattern 'v0.8.18 Docs, Help, and Skills Synchronization' docs/plan/development-phases.md

for skill in skills/*/SKILL.md; do
  require_pattern 'operon .*--help|operon <command> --help|CLI help is the source of truth' "$skill"
done

help_commands=(
  "--help"
  "config --help"
  "config explain --help"
  "node --help"
  "node list --help"
  "node discover --help"
  "node resolve --help"
  "node ping --help"
  "init --help"
  "init config --help"
  "onboard --help"
  "capability --help"
  "capability list --help"
  "fs --help"
  "fs stat --help"
  "fs list --help"
  "fs read --help"
  "fs write --help"
  "fs mkdir --help"
  "fs rm --help"
  "fs rename --help"
  "fs copy --help"
  "fs truncate --help"
  "audit --help"
  "audit list --help"
  "audit show --help"
  "service --help"
  "service list --help"
  "service check --help"
  "service forward --help"
  "service forward-udp --help"
  "job --help"
  "job run --help"
  "job list --help"
  "job status --help"
  "job logs --help"
  "job stdin --help"
  "job cancel --help"
  "run --help"
  "graph --help"
  "graph run --help"
  "workflow --help"
  "workflow run --help"
  "trace --help"
  "trace show --help"
  "trace list --help"
  "mount --help"
  "completion --help"
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

echo "docs, help, and skills synchronization validation passed"
