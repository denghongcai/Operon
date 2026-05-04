#!/usr/bin/env bash
set -euo pipefail

rg -n 'rpc StreamExecLogs\(ExecIdRequest\) returns \(stream ExecLogStreamEvent\)' proto/operon/runtime.proto
rg -n 'message ExecLogSnapshot' proto/operon/runtime.proto
rg -n 'message ExecLogComplete' proto/operon/runtime.proto

rg -n 'pub struct StoreWriter' crates/operon-store/src/lib.rs
rg -n 'pub enum FsyncPolicy' crates/operon-store/src/lib.rs
rg -n 'pub fn append_record\(path: Option<&Path>, record: &serde_json::Value\) -> anyhow::Result<\(\)>' crates/operon-store/src/lib.rs

rg -n 'StoreWriter::new' crates/operond/src/main.rs
rg -n 'exec_log_snapshot_event' crates/operond/src/exec_runtime.rs
rg -n 'exec_log_complete_event' crates/operond/src/exec_runtime.rs
rg -n 'exec_log_snapshot_event' crates/operond/src/exec_service.rs
rg -n 'exec_log_complete_event' crates/operond/src/exec_service.rs

rg -n "\\[target\\.'cfg\\(target_os = \"linux\"\\)'\\.dependencies\\]" crates/operon-mount/Cargo.toml
rg -n '^#\[cfg\(target_os = "linux"\)\]' crates/operon-mount/src/lib.rs
rg -n '^pub mod mount_core;' crates/operon-mount/src/lib.rs
rg -n '^pub trait RemoteFs' crates/operon-mount/src/mount_core.rs

rg -n 'export type ExecLogStreamEvent' packages/sdk-js/src/index.ts
rg -n 'streamExecLogEvents' packages/sdk-js/src/index.ts
rg -n 'PROTOCOL_VERSION: &str = "v0.14.0"' crates/operon-protocol/src/lib.rs

cargo test -p operon-store --locked
cargo test -p operond --locked
if [[ "${OPERON_SKIP_SDK_TESTS:-}" == "1" ]]; then
  echo "skipping @operon/sdk tests; TypeScript CI already covers them"
else
  pnpm --filter @operon/sdk test
fi

echo "runtime-boundary validation passed"
