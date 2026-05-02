# AGENTS.md

Guidance for agents working in this repository.

## Project Direction

Operon is an AI-native capability runtime for distributed computers connected by existing private networks.

It is not a VPN, networking mesh, remote desktop, cloud computer, file sync tool, or SSH wrapper. Operon should run on top of mature connectivity layers such as Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, Kubernetes networking, or manually configured private endpoints.

Operon owns:

- capability runtime
- policy
- execution graph
- agent / CLI / SDK
- audit
- AI-native tool interface

Operon should not own:

- NAT traversal
- relay network infrastructure
- VPN/device mesh IP assignment
- global routing
- subnet routing
- packet-level network policy

## Key Design Documents

- `docs/plan/development-phases.md`
  - Authoritative phase plan and phase status tracker.
  - Every completed task must update this document with what changed and which phase advanced.

- `docs/plan/v0.2-acceptance.md`
  - v0.2 acceptance scope, validation commands, and known limits.

- `docs/plan/v0.3-acceptance.md`
  - v0.3 acceptance scope, validation commands, and known limits.

- `docs/plan/v0.4-acceptance.md`
  - v0.4 acceptance scope, validation commands, service capability limits, and trace/audit UX expectations.

- `docs/plan/v0.5-acceptance.md`
  - v0.5 acceptance scope for the gRPC runtime protocol migration.

- `docs/plan/v0.5.1-cleanup-acceptance.md`
  - v0.5.1 cleanup scope for removing the HTTP runtime facade after gRPC.

- `docs/plan/v0.6-acceptance.md`
  - v0.6 acceptance scope for Linux-only real FUSE mount support.

- `docs/plan/v0.6.1-acceptance.md`
  - v0.6.1 acceptance scope for Linux-only write FUSE mount support.

- `docs/plan/v0.6.2-cli-fs-cleanup-acceptance.md`
  - v0.6.2 cleanup scope for CLI fs mutation command alignment.

- `docs/plan/v0.6.3-fs-copy-acceptance.md`
  - v0.6.3 acceptance scope for same-node fs copy in protocol, CLI, and SDK.

- `docs/plan/v0.6.4-onboard-acceptance.md`
  - v0.6.4 acceptance scope for guided first-run configuration.

- `docs/plan/v0.6.5-unified-config-acceptance.md`
  - v0.6.5 acceptance scope for unified `config.yaml`.

- `docs/plan/v0.6.6-acceptance.md`
  - v0.6.6 acceptance scope for release hardening, job environment isolation,
    graph audit context, streaming clients, and runtime crate boundaries.

- `docs/plan/v0.6.7-acceptance.md`
  - v0.6.7 acceptance scope for Linux job process-group termination,
    binary-safe job logs, and explicit async CLI runtime ownership.

- `docs/plan/v0.6.8-acceptance.md`
  - v0.6.8 acceptance scope for gRPC schema-level protocol stabilization.

- `docs/plan/v0.6.8-release-cleanup.md`
  - v0.6.8 final release cleanup scope for runtime retention, CI validation,
    config docs, and protocol version alignment.

- `docs/plan/v0.6.9-cli-contract-cleanup.md`
  - v0.6.9 cleanup scope for CLI script contracts, job failure exits, JSON and
    quiet output behavior, health version reporting, and starter config files.

- `docs/plan/v0.6.10-runtime-hardening.md`
  - v0.6.10 hardening scope for store durability, terminal job audit, fs range
    validation, pagination metadata, spawn errors, and LAN discovery removal
    handling.

- `docs/plan/v0.6.11-maintainability-governance.md`
  - v0.6.11 maintainability scope for daemon support-module splits,
    poisoned-lock handling, Linux-only mount dependency gating, and governance
    validation.

- `docs/plan/v0.6.12-runtime-boundary-stabilization.md`
  - v0.6.12 runtime boundary scope for job-log streaming envelopes, store
    writer boundaries, daemon persistence visibility, and Linux mount adapter
    crate boundaries.

- `docs/plan/v0.7-acceptance.md`
  - v0.7 acceptance scope for service metadata, health checks, and explicit
    local service forwarding.

- `docs/plan/v0.7.1-udp-datagram-forwarding.md`
  - Planned v0.7.1 scope for UDP/datagram forwarding as a separate protocol
    from TCP service forwarding.

- `docs/plan/v0.8-acceptance.md`
  - v0.8 acceptance scope for the Agent Skills Pack.

- `docs/plan/v0.8.3-read-range-release-cleanup.md`
  - v0.8.3 scope for gRPC `ReadFileRange`, Linux FUSE random-read efficiency,
    and release/package/protocol version policy cleanup.

- `docs/plan/v0.8.4-runtime-cli-modularization.md`
  - v0.8.4 scope for behavior-preserving `operond` and `operon-cli`
    modularization before provider discovery work resumes.

- `docs/plan/v0.8.6-runtime-cli-client-modularization.md`
  - v0.8.6 scope for shared Rust gRPC client helpers, non-fs CLI command
    modules, Linux mount adapter modules, daemon runtime internals, and SDK
    public API alignment.

- `docs/plan/v0.8.7-fs-service-reuse-cleanup.md`
  - v0.8.7 cleanup scope for daemon filesystem service authorization, path
    resolution, and audit helper reuse.

- `docs/plan/v0.8.8-fs-stream-handler-cleanup.md`
  - v0.8.8 cleanup scope for moving daemon full-file filesystem stream
    handlers behind the filesystem service module boundary.

- `docs/plan/v0.8.9-service-tunnel-boundary-cleanup.md`
  - v0.8.9 cleanup scope for moving daemon service tunnel open/handshake
    logic behind the service forwarding module boundary.

- `docs/plan/v0.8.10-mount-lock-hardening.md`
  - v0.8.10 cleanup scope for returning errno from Linux FUSE mount callbacks
    instead of panicking on poisoned inode-table locks.

- `docs/plan/v0.9-acceptance.md`
  - v0.9 acceptance scope for non-LAN provider discovery adapters.

- `docs/architecture/runtime-api.md`
  - Current gRPC runtime API shape, CLI/SDK interface boundary, and service capability boundary.

- `PROTOCOL.md`
  - Direct gRPC protocol integration guide for clients that do not use an Operon SDK.

- `README.md`
  - Public-facing project positioning and high-level architecture.
  - Current framing: "AI-native capability runtime over existing private networks."

- `docs/decisions/computer-mesh-operon-summary.md`
  - Product and concept decision summary.
  - Covers the shift from "computer mesh" to Operon, core abstractions, AI-native positioning, network boundary, MVP scope, and open questions.

- `docs/architecture/technology-and-protocol-decisions.md`
  - Technical architecture decisions.
  - Covers Rust daemon core, TypeScript SDK, gRPC streaming protocol, CLI/SDK
    interfaces, service forwarding, provider adapters, distribution targets,
    and non-goals.

- `docs/dicussions/computer-mesh-operon.md`
  - Raw archived discussion that led to the current direction.
  - Keep the existing folder spelling unless the repo intentionally migrates it.

## Current Architecture Decisions

- Core daemon: Rust.
- SDK: TypeScript.
- CLI: Rust.
- Current core daemon protocol: gRPC with streaming.
- Human, ops, and script interface: `operon` CLI, including `--json`.
- TypeScript SDK interface: `nice-grpc`.
- HTTP runtime facade was removed in v0.5.1; do not reintroduce direct HTTP runtime APIs.
- Completed protocol milestone: v0.5 gRPC runtime protocol migration.
- Completed cleanup milestone: v0.5.1 removed the HTTP runtime facade and added `PROTOCOL.md`.
- Completed mount milestone: v0.6 Linux-only read-only real FUSE mount.
- Completed mount milestone: v0.6.1 Linux-only write-through FUSE mount.
- Completed cleanup milestone: v0.6.2 CLI fs mutation command alignment.
- Completed fs milestone: v0.6.3 same-node fs copy for protocol, CLI, and SDK.
- Completed onboarding milestone: v0.6.4 guided first-run setup through `operon onboard`.
- Completed config milestone: v0.6.5 unified `config.yaml` through `operon-config`.
- Completed hardening milestone: v0.6.6 workspace containment, isolated job
  environment construction, graph audit context, streaming client cleanup, and
  runtime helper crate boundaries.
- Completed runtime cleanup milestone: v0.6.7 process lifecycle, binary job
  logs, and explicit async CLI runtime.
- Completed protocol stabilization milestone: v0.6.8 typed runtime enums,
  proto3 optional presence, streaming request envelopes, paginated list APIs,
  and active proto surface pruning.
- Completed release cleanup milestone: v0.6.8 runtime retention bounding,
  current CI validation coverage, unified config docs, and protocol version
  alignment.
- Completed hardening milestone: v0.6.10 runtime store, audit, fs range,
  pagination, spawn-error, and LAN discovery hardening.
- Completed governance milestone: v0.6.11 maintainability governance split
  daemon support modules, removed direct poisoned-lock panics from `operond`
  main, gated mount dependencies to Linux, and added validation coverage.
- Completed runtime boundary milestone: v0.6.12 protocol streaming envelopes,
  append-only store writer boundaries, daemon runtime helper ownership cleanup,
  Linux mount adapter boundaries, and validation coverage.
- Completed service milestone: v0.7 service metadata, health checks, and
  explicit local forwarding for policy-allowed services.
- Completed service milestone: v0.7.1 UDP/datagram forwarding with a separate
  datagram protocol.
- Completed agent skills milestone: v0.8 repo-local skills, `operon config
  explain`, public CLI help validation, and CI coverage for agent usage
  guidance.
- Completed CLI cleanup milestone: shell completion generation via
  `operon completion <shell>` and completion setup guidance in `operon
  onboard`.
- Completed test coverage milestone: unit coverage audit, compiled-binary CLI
  integration tests, real-daemon integration coverage script, and CI validation
  for the core config/node/capability/fs/job/service/audit/graph/trace flows.
- Completed runtime cleanup milestone: audit timestamps are `u64` end-to-end,
  CLI private-file/token helpers are shared, UDP service forwarding awaits
  aborted local read tasks, service check/forward are explicitly authorized,
  and larger policy/protocol hardening items are recorded in
  `docs/plan/v0.8.2-runtime-cleanup.md`.
- Completed protocol cleanup milestone: v0.8.3 added `ReadFileRange` for
  efficient FUSE random reads, kept `ReadFile` as the streaming full-file API,
  documented release/package/protocol version policy, and bumped
  `PROTOCOL_VERSION` to `v0.8.3`.
- Completed maintainability cleanup milestone: v0.8.4 extracted daemon
  filesystem handlers and pagination helpers, plus CLI output, target parsing,
  and fs command handlers. Job runtime, service forwarding, audit helpers, and
  non-fs CLI command families remain follow-up modularization work.
- Completed core domain boundary milestone: v0.8.5 split `operon-core` into
  runtime, fs, job, service, policy, audit, discovery, and trace modules while
  keeping root-level public re-exports for compatibility.
- Completed maintainability milestone: v0.8.6 added shared Rust gRPC client
  helpers, split non-fs CLI commands, split Linux mount modules, extracted
  daemon auth/audit/state/job/service internals, added graph/workflow aliases,
  aligned the TypeScript SDK public surface, and added CI validation coverage.
- Completed cleanup milestone: v0.8.7 reduced daemon filesystem service
  authorization, path resolution, and failed-audit handling duplication while
  preserving existing fs operation behavior.
- Completed cleanup milestone: v0.8.8 moved daemon full-file filesystem stream
  read/write behavior behind `fs_service.rs`, leaving `operond/src/main.rs`
  responsible only for gRPC auth, audit context scoping, and delegation for
  those RPCs.
- Completed cleanup milestone: v0.8.9 moved daemon TCP and UDP service tunnel
  open/handshake logic behind `service_forward.rs`, leaving
  `operond/src/main.rs` responsible only for gRPC auth, audit context scoping,
  and delegation for those RPCs.
- Completed hardening milestone: v0.8.10 replaced Linux FUSE mount inode-table
  write-lock panics with helper-mediated errors that callbacks return as
  errno responses.
- Next planned milestone: v0.9 non-LAN provider discovery.
- Browser management UI and CLI TUI console are no longer planned product
  surfaces.
- Network layer: outsourced to Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, Kubernetes, or manual endpoints.
- v0.1 should assume nodes are already reachable over TCP.
- Provider adapters should resolve/discover endpoints, not implement connectivity.
- Capability authorization must remain inside Operon even when network access is already allowed.
- Service / port capability includes configured metadata, TCP health checking,
  and explicit local forwarding for policy-allowed services over existing
  Operon node connections; it must not become VPN, relay networking, NAT
  traversal, mesh IP assignment, global routing, or unmanaged port exposure.
- UDP/datagram forwarding uses a separate datagram-oriented protocol instead of
  reusing the TCP byte-stream tunnel.
- v0.8 produced skills that teach agents how to use Operon; it did not add an
  agent runtime or a parallel control plane.
- v0.8 improved CLI self-description for agents: all public command help paths
  are validated, and `operon config explain` provides a config interpretation
  view.
- v0.8 skills teach scenarios, command selection, safety checks, and
  when to inspect audit/trace output. They should direct agents to use
  `operon <command> --help` for exact syntax instead of duplicating every flag.
- `operon onboard` is only a guided wrapper over normal config files and CLI setup primitives; keep command-style configuration available for scripts and CI.
- `config.yaml` is the only supported runtime config format. CLI and daemon settings can be separate sections, but they should stay under the same config entrypoint with file references for sensitive values.

## First MVP Boundary

Prioritize:

- node identity
- authenticated RPC
- manually configured reachable endpoints
- capability discovery
- filesystem read/write
- process/job execution
- execution trace
- permission policy
- CLI
- minimal SDK
- service metadata, health checks, and explicit local forwarding
- trace/audit CLI inspection

Defer:

- screen streaming
- audio
- remote desktop
- clipboard sync
- full mount layer
- full file sync engine
- complex secret manager
- plugin marketplace
- NAT traversal
- unmanaged port forwarding and proxying
- relay network
- VPN/device mesh IP assignment

## Working Notes

- Preserve the distinction between network access and capability access.
- Do not reintroduce Operon-owned transport/mesh/VPN responsibilities without updating the decision docs first.
- Prefer adding or updating decision records when changing product scope or architecture boundaries.
- Keep README language aligned with the decision docs.
- After every task, update `docs/plan/development-phases.md` before finishing.
- Phase updates must state which phase changed, what was completed, and what remains.
- If implementation changes the phase scope, update the phase text itself instead of only adding a note.
- If no phase status changed, explicitly record that in the relevant phase or explain it in the final response.
- Latest phase status update: v0.7 completed service tunnel protocol, CLI local
  forwarding, TypeScript SDK tunnel support, service forwarding docs, and CI
  validation coverage. Nothing remains in v0.7.
- Latest phase status update: v0.7.1 completed UDP service protocol support,
  `OpenServiceDatagramTunnel`, daemon UDP peer sessions, CLI `forward-udp`,
  TypeScript SDK datagram helpers, documentation updates, and CI validation.
  Nothing remains in v0.7.1.
- Latest phase status update: v0.6.11 completed daemon support-module splits,
  poisoned-lock handling, Linux-only mount dependency gating, and CI governance
  validation. Larger domain splits remain future work.
- Latest phase status update: v0.6.12 completed Runtime Boundary Stabilization.
  `StreamJobLogs` now uses event envelopes, `operon-store` exposes an
  append-only writer boundary, daemon persistence failures surface at runtime
  boundaries, `operon-mount` is a Linux FUSE adapter crate boundary, CI
  includes the v0.6.12 validation script, and the post-release documentation
  drift pass aligned current docs with the v0.6.12 runtime. Nothing remains in
  v0.6.12.
- Latest phase status update: v0.8.3 completed Read Range and Release Cleanup.
  `ReadFileRange` is implemented in proto, daemon, Linux mount, SDK, docs, and
  CI validation. Nothing remains in v0.8.3.
- Latest phase status update: v0.8.4 completed the first Runtime and CLI
  Modularization pass. Daemon fs and pagination logic plus CLI fs/output/target
  logic are extracted and validated. Job/service/audit and non-fs CLI command
  extraction remains future maintainability work.
- Latest phase status update: v0.8.5 completed. `operon-core` domain modules
  are split, compatibility re-exports remain in place, CI has a dedicated
  v0.8.5 validation script, and no behavior or schema work remains in this
  phase.
- Latest phase status update: v0.8.6 completed Runtime, CLI, and Client
  Modularization. Shared Rust gRPC client helpers, non-fs CLI command modules,
  Linux mount modules, daemon auth/audit/state/job/service internals,
  graph/workflow aliases, SDK public API alignment, `fs read --output --json`
  summary output, and low-risk validation shell helpers are complete. Nothing
  remains in v0.8.6.
- Latest phase status update: v0.8.7 completed Filesystem Service Reuse
  Cleanup. `fs_service.rs` now uses helper boundaries for authorization,
  workspace path resolution, and failed-audit handling. Nothing remains in
  v0.8.7.
- Latest phase status update: v0.8.8 completed Filesystem Stream Handler
  Cleanup. Full-file `ReadFile` and `WriteFile` stream behavior now lives in
  `fs_service.rs`, and the runtime router delegates those RPCs. Nothing
  remains in v0.8.8.
- Latest phase status update: v0.8.9 completed Service Tunnel Boundary
  Cleanup. TCP and UDP service tunnel target parsing, authorization, protocol
  checks, audit handling, and connection setup now live in
  `service_forward.rs`, and the runtime router delegates those RPCs. Nothing
  remains in v0.8.9.
- Latest phase status update: v0.8.10 completed Mount Lock Hardening.
  `operon-mount` FUSE callbacks no longer panic on poisoned inode-table write
  locks; they return errno responses or propagated mount errors. Nothing
  remains in v0.8.10.
