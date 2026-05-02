#!/usr/bin/env bash
set -euo pipefail

for module in defaults grpc_status lan_advertise locks store_config; do
  test -f "crates/operond/src/${module}.rs"
done

if rg -n 'expect\(".*poisoned' crates/operond/src/main.rs; then
  echo "operond main still contains direct poisoned-lock expect calls" >&2
  exit 1
fi

rg -n 'operon-mount = \{ path = "../operon-mount" \}' crates/operon-cli/Cargo.toml
rg -n '\[target.'\''cfg\(target_os = "linux"\)'\''.dependencies\]' crates/operon-cli/Cargo.toml
rg -n 'operon mount is only supported on Linux' crates/operon-cli/src/commands/mount.rs

cargo test -p operond --locked locks::tests::poisoned_lock_returns_internal_status
cargo test -p operond --locked lan_advertise::tests::unspecified_addresses_advertise_localhost
cargo check -p operon-cli --locked
