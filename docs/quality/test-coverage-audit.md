# Test Coverage Audit

Status: Updated for v0.8.1.

This audit focuses on meaningful coverage of Operon's core contracts rather
than enforcing a line-percentage gate. The current project has three useful
test layers:

- Rust unit tests in every Rust crate.
- TypeScript SDK tests under `packages/sdk-js`.
- validation scripts that exercise real CLI, daemon, gRPC, mount, service, and
  Docker-node workflows.

## Unit Coverage

Current Rust crate coverage by responsibility:

| Crate | Covered behaviors | Remaining emphasis |
| --- | --- | --- |
| `operon-core` | DTO serialization, policy YAML, service policy, graph YAML | keep enum and wire-name tests when protocol evolves |
| `operon-config` | unified config loading, endpoint resolution, auth serialization | add provider discovery config tests when v0.9 lands |
| `operon-fs` | workspace path containment, symlink behavior, policy scope, stable ids | add more Linux fd/openat2 tests if containment moves lower |
| `operon-process` | job policy, environment allowlist, preserve-env mode | add more process-group tests around platform differences |
| `operon-store` | append-only JSONL writer, fsync toggle, symlink rejection | add corruption/recovery tests if a richer store is added |
| `operon-network` | LAN discovery removal handling | add provider API mocking in v0.9 |
| `operon-protocol` | page tokens and protocol version | keep conversion tests aligned with proto changes |
| `operon-mount` | inode table, path validation, runtime wrapper, Linux mount boundaries | real FUSE behavior remains in Linux validation scripts |
| `operond` | daemon policy, fs, job lifecycle, audit, pagination, store path, locks | keep high-risk runtime behavior here instead of only scripts |
| `operon-cli` | command parsing helpers, config explain, onboard, completion command model | integration tests now cover binary behavior |

## Integration Coverage

The repository now includes two additional integration checks:

- `crates/operon-cli/tests/cli_static_integration.rs`
  - validates the compiled `operon` binary for help, completion generation,
    `init config`, `config explain --json`, and onboard completion guidance.
- `scripts/verify-v0.8.1-integration-coverage.sh`
  - starts a real `operond`.
  - exercises `config explain`, node ping, capability list, service health,
    completion generation, fs write/read/copy/truncate/rm, job run/log
    retrieval, execution graph run, trace inspection, and filtered audit.
  - lists workspace tests and ensures the core crates have registered tests.

Existing validation scripts continue to cover Docker two-node behavior, Linux
mount behavior, runtime hardening, protocol boundaries, service forwarding,
UDP/datagram forwarding, and agent skills.

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
