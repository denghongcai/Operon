#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

rg -n 'rpc ReadFileRange\(FsReadRangeRequest\) returns \(FileChunk\)' proto/operon/runtime.proto
rg -n 'message FsReadRangeRequest' proto/operon/runtime.proto
rg -n 'async fn read_file_range' crates/operond/src/main.rs
rg -n '^pub\(crate\) async fn read_range' crates/operond/src/fs_service.rs
rg -n 'fs_service::read_range' crates/operond/src/main.rs
rg -n 'read_file_range\(operon_grpc_client::request' crates/operon-mount/src/remote_client.rs
rg -n 'readFileRangeBytes' packages/sdk-js/src/index.ts
rg -n 'ReadFileRange' PROTOCOL.md docs/architecture/runtime-api.md docs/architecture/technology-and-protocol-decisions.md
rg -n 'PROTOCOL_VERSION: &str = "v0.9.7"' crates/operon-protocol/src/lib.rs

python - <<'PY'
from pathlib import Path

text = Path("crates/operon-mount/src/remote_client.rs").read_text()
impl_start = text.index("impl RemoteFs for GrpcRemoteFs")
start = text.index("    fn read_range(&self, path: &str, offset: u64, size: u32)", impl_start)
end = text.index("    fn write_range(&self, path: &str, offset: u64, data: &[u8])", start)
body = text[start:end]
if ".read_file_range(" not in body:
    raise SystemExit("GrpcRemoteFs::read_range does not call read_file_range")
if ".read_file(" in body:
    raise SystemExit("GrpcRemoteFs::read_range still calls full read_file stream")

development = Path("DEVELOPMENT.md").read_text()
if "VERSION=v0.6.12" in development or "git tag v0.6.12" in development:
    raise SystemExit("DEVELOPMENT still hard-codes a stale v0.6.12 release command")
for required in [
    "GitHub release tags identify shipped binary bundles",
    "`PROTOCOL_VERSION` identifies the public gRPC wire/API compatibility line",
]:
    if required not in development:
        raise SystemExit(f"DEVELOPMENT missing version policy text: {required}")
PY

cargo test -p operon-protocol --locked
cargo test -p operond --locked
cargo test -p operon-mount --locked
pnpm --filter @operon/sdk test

echo "v0.8.3 read-range and release cleanup validation passed"
