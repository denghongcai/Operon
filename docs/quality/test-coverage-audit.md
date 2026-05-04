# Test Coverage Audit

Status: Updated for v0.13.6.

This audit focuses on meaningful coverage of Operon's core contracts rather
than enforcing a line-percentage gate. The current project has three useful
test layers:

- Rust unit tests in every Rust crate.
- TypeScript SDK tests under [`packages/sdk-js`](../../packages/sdk-js).
- validation scripts that exercise real CLI, daemon, gRPC, mount, service, and
  Docker-node workflows.

## Unit Coverage

Current Rust crate coverage by responsibility:

| Crate | Covered behaviors | Remaining emphasis |
| --- | --- | --- |
| [`operon-core`](../../crates/operon-core) | DTO serialization, policy YAML, service policy, graph YAML | keep enum and wire-name tests when protocol evolves |
| [`operon-config`](../../crates/operon-config) | unified config loading, endpoint resolution, auth serialization, unknown-field warnings | keep endpoint-only config tests aligned with v0.9 acceptance |
| [`operon-fs`](../../crates/operon-fs) | workspace path containment, symlink behavior, policy scope, stable ids | add more Linux fd/openat2 tests if containment moves lower |
| [`operon-process`](../../crates/operon-process) | exec policy, environment allowlist, preserve-env mode | add more process-group tests around platform differences |
| [`operon-store`](../../crates/operon-store) | append-only JSONL writer, fsync toggle, symlink rejection, persisted audit/exec-log loading with `TempDir` cleanup | add corruption/recovery tests if a richer store is added |
| [`operon-network`](../../crates/operon-network) | LAN mDNS discovery removal handling, endpoint record conversion, deterministic TCP/UDP service-check behavior | keep live mDNS behavior out of default tests unless it can be made deterministic |
| [`operon-protocol`](../../crates/operon-protocol) | page tokens and protocol version | keep conversion tests aligned with proto changes |
| [`operon-grpc-client`](../../crates/operon-grpc-client) | URI normalization, auth/context metadata, write/stdin stream chunking, connection deadline helper | add local tonic-server coverage if new client RPC helpers carry richer behavior |
| [`operon-mount`](../../crates/operon-mount) | inode table, path validation, errno mapping, FUSE helper behavior, Linux mount boundaries | live kernel FUSE behavior remains in Linux validation scripts |
| [`operond`](../../crates/operond) | daemon policy, fs, exec lifecycle, audit, pagination, store path, locks | keep high-risk runtime behavior here instead of only scripts |
| [`operon-cli`](../../crates/operon-cli) | command parsing helpers, config explain, onboard, completion command model, negative-path binary behavior | keep script-facing JSON/quiet/exit-code contracts covered when CLI paths change |

## Integration Coverage

The repository now includes two additional integration checks:

- [`crates/operon-cli/tests/cli_static_integration.rs`](../../crates/operon-cli/tests/cli_static_integration.rs)
  - validates the compiled `operon` binary for help, completion generation,
    `init config`, `config explain --json`, and onboard completion guidance.
  - validates negative-path behavior for unknown commands, missing required
    arguments, malformed config files, and invalid endpoint schemes.
- [`scripts/verify-v0.8.1-integration-coverage.sh`](../../scripts/verify-v0.8.1-integration-coverage.sh)
  - starts a real `operond`.
  - exercises `config explain`, node ping, capability list, service health,
    completion generation, fs write/read/copy/truncate/rm, exec run/log
    retrieval, execution graph run, trace inspection, and filtered audit.
  - lists workspace tests and ensures the core crates have registered tests.

Existing validation scripts continue to cover Docker two-node behavior, Linux
mount behavior, runtime hardening, protocol boundaries, service forwarding,
UDP/datagram forwarding, agent skills, and docs/help/skills synchronization.
[`scripts/verify-v0.13.6-test-hardening.sh`](../../scripts/verify-v0.13.6-test-hardening.sh)
adds a focused gate for the v0.13.6 service-check, gRPC-client, mount-helper,
CLI negative-path, duplicate token-test cleanup, and `TempDir` cleanup work.

## Known Limits

- Linux FUSE live mount behavior is still validated by Linux scripts because
  kernel FUSE callbacks are not portable unit-test targets.
- `operon-network` avoids live mDNS unit tests because host multicast behavior
  is not deterministic enough for the default test suite.
- The project still intentionally avoids a numeric line-coverage threshold; the
  acceptance target is contract coverage at the unit, compiled-binary, and
  validation-script layers.

## Coverage Policy

For this phase, coverage is accepted when:

- every Rust crate has at least one targeted unit test group.
- CLI behavior that users or agents call directly has a compiled-binary
  integration test.
- core daemon behavior is exercised through a real daemon process, not only
  through helper functions.
- validation scripts are included in CI so regressions block the default branch.

Line coverage tooling such as `cargo llvm-cov` can be added later if the team
wants a numeric threshold, but it should not replace contract-level runtime
tests.
