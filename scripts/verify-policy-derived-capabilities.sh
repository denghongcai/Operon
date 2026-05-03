#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/lib/validation.sh
source "$ROOT/scripts/lib/validation.sh"

require_file docs/plan/v0.9.2-policy-derived-capabilities.md
require_pattern 'capabilities_from_policy' crates/operond/src/defaults.rs
require_pattern 'capabilities_from_policy\(&node.id, &policy\)' crates/operond/src/main.rs
require_pattern 'policy_capabilities_do_not_advertise_unconfigured_policy_surfaces' crates/operond/src/defaults.rs
require_pattern 'policy_capabilities_reflect_configured_mounts_execs_and_services' crates/operond/src/defaults.rs
require_pattern 'v0.9.2 Policy-Derived Capability Discovery' docs/plan/development-phases.md
require_pattern 'scripts/verify-policy-derived-capabilities.sh' DEVELOPMENT.md
require_pattern 'scripts/verify-policy-derived-capabilities.sh' .github/workflows/ci.yml
require_pattern 'policy-derived' README.md

reject_pattern 'fn default_capabilities' crates/operond/src/defaults.rs
reject_pattern '"service:default"' crates/operond/src/service_forward.rs
reject_pattern 'cloud-a/process:default run' docs/plan/development-phases.md

cargo test -p operond --locked policy_capabilities_do_not_advertise_unconfigured_policy_surfaces
cargo test -p operond --locked policy_capabilities_reflect_configured_mounts_execs_and_services
bash scripts/verify-docs-help-skills-sync.sh

echo "policy-derived capability validation passed"
