#!/usr/bin/env bash
set -euo pipefail

rg -n 'rpc StreamJobLogs\(JobIdRequest\) returns \(stream JobLogStreamEvent\)' proto/operon/runtime.proto
rg -n 'message JobLogSnapshot' proto/operon/runtime.proto
rg -n 'message JobLogComplete' proto/operon/runtime.proto

rg -n 'pub struct StoreWriter' crates/operon-store/src/lib.rs
rg -n 'pub enum FsyncPolicy' crates/operon-store/src/lib.rs
rg -n 'pub fn append_record\(path: Option<&Path>, record: &serde_json::Value\) -> anyhow::Result<\(\)>' crates/operon-store/src/lib.rs

rg -n 'StoreWriter::new' crates/operond/src/main.rs
rg -n 'job_log_snapshot_event' crates/operond/src/main.rs
rg -n 'job_log_complete_event' crates/operond/src/main.rs

rg -n "\\[target\\.'cfg\\(target_os = \"linux\"\\)'\\.dependencies\\]" crates/operon-mount/Cargo.toml
rg -n '^#!\[cfg\(target_os = "linux"\)\]' crates/operon-mount/src/lib.rs

rg -n 'export type JobLogStreamEvent' packages/sdk-js/src/index.ts
rg -n 'streamJobLogEvents' packages/sdk-js/src/index.ts
rg -n 'PROTOCOL_VERSION: &str = "v0.6.12"' crates/operon-protocol/src/lib.rs

cargo test -p operon-store --locked
cargo test -p operond --locked
pnpm --filter @operon/sdk test

echo "v0.6.12 runtime-boundary validation passed"
