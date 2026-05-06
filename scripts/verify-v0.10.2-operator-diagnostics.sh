#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.10.2-operator-diagnostics.md
require_pattern 'Status: Completed' docs/plan/v0.10.2-operator-diagnostics.md
require_pattern 'v0.10.2 Operator Diagnostics' docs/plan/development-phases.md
require_pattern 'No v0.10.2 work remains' docs/plan/development-phases.md

require_pattern 'Command::Doctor' crates/operon-cli/src/cli_dispatch.rs
require_pattern 'pub\(crate\) mod doctor' crates/operon-cli/src/commands/mod.rs
require_pattern 'struct DoctorReport' crates/operon-cli/src/commands/doctor.rs
require_pattern 'from_str_with_warnings' crates/operon-cli/src/commands/doctor.rs
require_pattern 'health_and_node' crates/operon-cli/src/commands/doctor.rs
require_pattern 'explain_capability' crates/operon-cli/src/commands/doctor.rs
require_pattern 'check_service' crates/operon-cli/src/commands/doctor.rs
require_pattern 'operon doctor' README.md DEVELOPMENT.md skills/operon-cli-ops/SKILL.md

if ! output="$(cargo run -q -p operon-cli -- doctor --help 2>&1)"; then
  echo "operon doctor --help failed" >&2
  echo "$output" >&2
  exit 1
fi
grep -q 'diagnostics' <<<"$output"

tmp_config="$(mktemp)"
cat >"$tmp_config" <<'YAML'
version: 1
unexpected_root: true
client:
  nodes: {}
YAML

cargo run -q -p operon-cli -- --config "$tmp_config" --json doctor \
  | python3 -c 'import json,sys; data=json.load(sys.stdin); assert data["config_warnings"] == ["unexpected_root"]; assert data["nodes"] == []'
rm -f "$tmp_config"

cargo test -p operon-cli --locked doctor
bash scripts/verify-docs-help-skills-sync.sh

echo "v0.10.2 operator diagnostics validation passed"
