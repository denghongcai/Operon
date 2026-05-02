#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

test -f crates/operond/src/fs_service.rs
test -f crates/operond/src/pagination.rs
test -f crates/operon-cli/src/commands/fs.rs
test -f crates/operon-cli/src/output.rs
test -f crates/operon-cli/src/target.rs

rg -n '^mod fs_service;' crates/operond/src/main.rs
rg -n '^mod pagination;' crates/operond/src/main.rs
rg -n '^pub\(crate\) async fn read_range' crates/operond/src/fs_service.rs
rg -n '^pub\(crate\) fn paginate_items' crates/operond/src/pagination.rs
rg -n '^pub\(crate\) async fn stat' crates/operon-cli/src/commands/fs.rs
rg -n '^pub\(crate\) struct OutputMode' crates/operon-cli/src/output.rs
rg -n '^pub\(crate\) fn parse_node_path' crates/operon-cli/src/target.rs

if rg -n '^fn paginate_items|^async fn grpc_fs_|^fn validate_write_chunk|^fn checked_file_end' crates/operond/src/main.rs; then
  echo "operond main still owns extracted fs/pagination helpers" >&2
  exit 1
fi

if rg -n '^async fn fs_|^fn parse_node_path|^pub\(crate\) fn print_json|^pub\(crate\) struct OutputMode' crates/operon-cli/src/main.rs; then
  echo "operon-cli main still owns extracted fs/output/target helpers" >&2
  exit 1
fi

cargo test -p operond --locked
cargo test -p operon-cli --locked

echo "v0.8.4 modularization validation passed"
