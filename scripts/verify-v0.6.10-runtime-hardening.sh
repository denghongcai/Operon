#!/usr/bin/env bash
set -euo pipefail

cargo test -p operon-store --locked
cargo test -p operon-network --locked
cargo test -p operon-protocol --locked list_conversions_preserve_page_tokens
cargo test -p operond --locked store_path_
cargo test -p operond --locked fs_range_validation_rejects_overflow_and_large_chunks
cargo test -p operond --locked mkdir_creates_missing_parent_directories
cargo test -p operond --locked finish_exec_records_terminal_audit_event
