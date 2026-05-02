#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for module in runtime fs job service policy audit discovery trace; do
  test -f "crates/operon-core/src/${module}.rs"
  rg -n "pub mod ${module};" crates/operon-core/src/lib.rs
done

for reexport in audit discovery fs job policy runtime service trace; do
  rg -n "pub use ${reexport}::\\*;" crates/operon-core/src/lib.rs
done

rg -n 'pub type NodeId' crates/operon-core/src/runtime.rs
rg -n 'pub struct FsReadRangeRequest' crates/operon-core/src/fs.rs
rg -n 'pub struct JobRecord' crates/operon-core/src/job.rs
rg -n 'pub struct ServiceDefinition' crates/operon-core/src/service.rs
rg -n 'pub struct PolicyConfig' crates/operon-core/src/policy.rs
rg -n 'pub struct AuditEvent' crates/operon-core/src/audit.rs
rg -n 'pub struct DiscoveryRecord' crates/operon-core/src/discovery.rs
rg -n 'pub struct ExecutionGraph' crates/operon-core/src/trace.rs
rg -n 'domain_module_paths_and_root_reexports_match' crates/operon-core/src/lib.rs

cargo test -p operon-core --locked

echo "v0.8.5 core domain module validation passed"
