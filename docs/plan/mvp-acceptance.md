# MVP Acceptance

This document defines the v0.1.0 acceptance baseline for Operon.

Historical note: this file captures the MVP acceptance snapshot. Later phases
replaced the temporary HTTP/JSON client, added auth, moved job/log/audit state
into bounded runtime/store structures, and expanded filesystem streaming. Use
`PROTOCOL.md`, `README.md`, and the latest completed phase entries for the
current runtime contract.

## Scope

Operon v0.1.0 proves that already-reachable machines can be exposed as an AI-operable capability runtime.

The MVP includes:

- manual node endpoint configuration
- daemon health and node metadata
- capability discovery
- filesystem stat/list/read/write inside configured mounts
- controlled job run/status/logs/cancel/timeout
- sequential execution graphs
- minimal policy checks
- audit records for allowed and denied operations
- TypeScript SDK workflow execution
- Docker two-node validation

## Non-Goals

The MVP does not include:

- VPN, relay, NAT traversal, or device mesh IP assignment
- automatic Cloudflare/Tailscale discovery
- HTTPS, mTLS, or bearer-token authentication
- durable job, trace, or audit storage
- FUSE/WinFsp mount support
- screen, clipboard, audio, or remote desktop capabilities
- graphical management UI
- secret injection
- complex policy language

## Required Validation

Run:

```bash
cargo fmt --check
cargo test --workspace --locked
cargo check --workspace --locked
cargo clippy --workspace --locked -- -D warnings
pnpm -r test
pnpm typecheck
scripts/verify-mvp-docker.sh
```

The Docker validation must prove:

- two `operond` containers start and respond to health checks
- CLI can list and ping both nodes
- CLI can list node capabilities
- fs write/stat/list/read work on both nodes
- path traversal outside the workspace is denied
- job run/log/status/cancel work on both nodes
- job timeout is represented as `TimedOut`
- policy denial is observable for an excessive job timeout
- audit output includes allowed and denied operations
- `operon run examples/docker-copy-and-run.yaml` produces a successful structured trace
- core Rust modules and the TypeScript SDK have unit test coverage for their main parsing, policy, and workflow behavior

## Release Checklist

Before tagging `v0.1.0`:

- `docs/plan/development-phases.md` marks MVP phases completed.
- `README.md` Quickstart runs from a fresh checkout with Docker available.
- CI passes Rust checks and TypeScript typecheck.
- Docker MVP validation passes locally.
- Known limitations remain documented in this file.
- Release notes mention that network connectivity is external to Operon.

## Known Limitations

- HTTP/JSON uses a temporary hand-written Rust client.
- Daemon endpoints are unauthenticated in v0.1.0.
- Job, trace, and audit state are in-memory.
- Filesystem read/write currently buffer text content.
- Execution graphs run sequentially and are not persisted.
- Policy is intentionally small and local-file based.
