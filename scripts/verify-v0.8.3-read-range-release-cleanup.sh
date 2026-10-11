#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

rg -n 'rpc ReadFileRange\(FsReadRangeRequest\) returns \(FileChunk\)' proto/operon/runtime.proto
rg -n 'message FsReadRangeRequest' proto/operon/runtime.proto
rg -n 'async fn read_file_range' crates/operond/src/runtime.rs
rg -n '^pub\(crate\) async fn read_range' crates/operond/src/fs_service.rs
rg -n 'fs_service::read_range' crates/operond/src/runtime.rs
rg -n 'read_file_range\(operon_grpc_client::request' crates/operon-mount/src/remote_client.rs
rg -n 'readFileRangeBytes' packages/sdk-js/src/index.ts
rg -n 'ReadFileRange' PROTOCOL.md docs/architecture/runtime-api.md docs/architecture/technology-and-protocol-decisions.md
rg -n 'PROTOCOL_VERSION: &str = "v0.16.14"' crates/operon-protocol/src/lib.rs

python - <<'PY'
from pathlib import Path

text = Path("crates/operon-mount/src/remote_client.rs").read_text()
impl_start = text.index("impl RemoteFs for GrpcRemoteFs")
start = text.index("    fn read_range(&self, path: &str, offset: u64, size: u32)", impl_start)
end = text.index("    fn write_range(&self, path: &str, offset: u64, data: &[u8])", start)
sync_body = text[start:end]
async_start = text.index("    pub(crate) async fn read_range_async(")
deadline_start = text.index("    pub(crate) async fn read_range_with_deadline(", async_start)
async_body = text[async_start:deadline_start]
deadline_body = text[deadline_start:impl_start]
if "self.read_range_async(path, offset, size)" not in sync_body:
    raise SystemExit("sync read_range does not delegate its exact range to shared async reads")
if "self.read_range_with_deadline(path, offset, size, self.read_deadline())" not in async_body:
    raise SystemExit("async read_range does not preserve its exact range and configured deadline")
if ".read_file_range(" not in deadline_body or "FsReadRangeRequest" not in deadline_body:
    raise SystemExit("shared range implementation does not call typed read_file_range RPC")
if ".read_file(" in sync_body + async_body + deadline_body:
    raise SystemExit("GrpcRemoteFs::read_range still calls full read_file stream")

development = Path("DEVELOPMENT.md").read_text()
if "VERSION=v0.6.12" in development or "git tag v0.6.12" in development:
    raise SystemExit("DEVELOPMENT still hard-codes a stale v0.6.12 release command")
for required in [
    "GitHub release tags identify shipped binary bundles",
    "`PROTOCOL_VERSION` must align with the public release tag",
]:
    if required not in development:
        raise SystemExit(f"DEVELOPMENT missing version policy text: {required}")
PY

cargo test -p operon-protocol --locked
cargo test -p operond --locked
cargo test -p operon-mount --locked
if [[ "${OPERON_SKIP_SDK_TESTS:-}" == "1" ]]; then
  echo "skipping @operon/sdk tests; TypeScript CI already covers them"
else
  pnpm --filter @operon/sdk test
fi

echo "v0.8.3 read-range and release cleanup validation passed"
