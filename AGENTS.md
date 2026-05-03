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
  - v0.6.6 acceptance scope for release hardening, exec environment isolation,
    graph audit context, streaming clients, and runtime crate boundaries.

- `docs/plan/v0.6.7-acceptance.md`
  - v0.6.7 acceptance scope for Linux exec process-group termination,
    binary-safe exec logs, and explicit async CLI runtime ownership.

- `docs/plan/v0.6.8-acceptance.md`
  - v0.6.8 acceptance scope for gRPC schema-level protocol stabilization.

- `docs/plan/v0.6.8-release-cleanup.md`
  - v0.6.8 final release cleanup scope for runtime retention, CI validation,
    config docs, and protocol version alignment.

- `docs/plan/v0.6.9-cli-contract-cleanup.md`
  - v0.6.9 cleanup scope for CLI script contracts, exec failure exits, JSON and
    quiet output behavior, health version reporting, and starter config files.

- `docs/plan/v0.6.10-runtime-hardening.md`
  - v0.6.10 hardening scope for store durability, terminal exec audit, fs range
    validation, pagination metadata, spawn errors, and LAN discovery removal
    handling.

- `docs/plan/v0.6.11-maintainability-governance.md`
  - v0.6.11 maintainability scope for daemon support-module splits,
    poisoned-lock handling, Linux-only mount dependency gating, and governance
    validation.

- `docs/plan/v0.6.12-runtime-boundary-stabilization.md`
  - v0.6.12 runtime boundary scope for exec-log streaming envelopes, store
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
  - v0.8.4 scope for behavior-preserving `operond` and [`operon-cli`](crates/operon-cli)
    modularization before endpoint discovery UX work resumes.

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

- `docs/plan/v0.8.11-cli-datagram-lock-hardening.md`
  - v0.8.11 cleanup scope for returning CLI errors instead of panicking on
    poisoned UDP datagram forwarding peer-state locks.

- `docs/plan/v0.8.12-daemon-datagram-invariant-cleanup.md`
  - v0.8.12 cleanup scope for removing the remaining daemon UDP datagram
    forwarding session invariant panic.

- `docs/plan/v0.8.13-production-panic-cleanup.md`
  - v0.8.13 cleanup scope for removing actionable production invariant panics
    from daemon exec-log append and Linux mount remote runtime access.

- `docs/plan/v0.8.14-onboard-invariant-cleanup.md`
  - v0.8.14 cleanup scope for returning a normal CLI error instead of
    panicking on a broken daemon onboarding token invariant.

- `docs/plan/v0.8.15-token-generation-panic-cleanup.md`
  - v0.8.15 cleanup scope for direct token hex encoding without a
    panic-style `String` formatting invariant.

- `docs/plan/v0.8.16-endpoint-model-simplification.md`
  - v0.8.16 cleanup scope for removing the provider abstraction from
    user-facing endpoint config, discovery output, CLI commands, and SDK types.

- `docs/plan/v0.8.17-config-unknown-field-warnings.md`
  - v0.8.17 cleanup scope for warning about unknown `config.yaml` fields
    without blocking startup or CLI commands.

- `docs/plan/v0.8.18-docs-help-skills-sync.md`
  - v0.8.18 cleanup scope for keeping docs, CLI help, repo-local skills, and
    AGENTS.md sync rules aligned with the implemented endpoint-only model.

- `docs/plan/v0.9-acceptance.md`
  - v0.9 acceptance scope for endpoint-only config and mDNS discovery UX.

- `docs/plan/v0.9.1-discovery-ux.md`
  - v0.9.1 scope for mDNS export conflict handling, optional discovery health
    checks, and external endpoint-only config generator guidance.

- `docs/plan/v0.9.2-policy-derived-capabilities.md`
  - v0.9.2 scope for policy-derived capability discovery instead of static
    default capability advertising.

- `docs/plan/v0.9.3-store-backed-audit-visibility.md`
  - v0.9.3 scope for restart-safe audit inspection by loading persisted audit
    events from the append-only store.

- `docs/plan/v0.9.4-runtime-hardening-consolidation.md`
  - v0.9.4 scope consolidating service health semantics, exec-log
    restart visibility, workspace traversal hardening, argv execution,
    config UX cleanup, and focused runtime maintainability cleanup.

- `docs/plan/v0.9.5-policy-language-hardening.md`
  - v0.9.5 scope for shared policy decision vocabulary, effective
    policy explain output, authorization consistency, clearer audit denial
    reasons, and validation coverage.

- `docs/plan/v0.9.6-capability-diagnostics.md`
  - v0.9.6 scope for daemon-owned policy diagnostics through gRPC, CLI, and
    SDK using the shared `PolicyDecision` vocabulary.

- `docs/plan/v0.9.7-runtime-api-hardening.md`
  - v0.9.7 scope for filesystem list pagination, runtime API documentation
    alignment, SDK streaming write behavior, and empty exec validation.

- `docs/plan/v0.10-exec-unification.md`
  - v0.10 scope for replacing the active `job` surface with the unified
    `exec` capability across protocol, daemon, CLI, SDK, docs, skills, and
    validation.

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
    interfaces, service forwarding, endpoint configuration, distribution targets,
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
- Completed config milestone: v0.6.5 unified `config.yaml` through [`operon-config`](crates/operon-config).
- Completed hardening milestone: v0.6.6 workspace containment, isolated exec
  environment construction, graph audit context, streaming client cleanup, and
  runtime helper crate boundaries.
- Completed runtime cleanup milestone: v0.6.7 process lifecycle, binary exec
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
  for the core config/node/capability/fs/exec/service/audit/graph/trace flows.
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
  and fs command handlers. Exec runtime, service forwarding, audit helpers, and
  non-fs CLI command families remain follow-up modularization work.
- Completed core domain boundary milestone: v0.8.5 split [`operon-core`](crates/operon-core) into
  runtime, fs, exec, service, policy, audit, discovery, and trace modules while
  keeping root-level public re-exports for compatibility.
- Completed maintainability milestone: v0.8.6 added shared Rust gRPC client
  helpers, split non-fs CLI commands, split Linux mount modules, extracted
  daemon auth/audit/state/exec/service internals, added graph/workflow aliases,
  aligned the TypeScript SDK public surface, and added CI validation coverage.
- Completed cleanup milestone: v0.8.7 reduced daemon filesystem service
  authorization, path resolution, and failed-audit handling duplication while
  preserving existing fs operation behavior.
- Completed cleanup milestone: v0.8.8 moved daemon full-file filesystem stream
  read/write behavior behind [`fs_service.rs`](crates/operond/src/fs_service.rs), leaving [`operond/src/main.rs`](crates/operond/src/main.rs)
  responsible only for gRPC auth, audit context scoping, and delegation for
  those RPCs.
- Completed cleanup milestone: v0.8.9 moved daemon TCP and UDP service tunnel
  open/handshake logic behind [`service_forward.rs`](crates/operond/src/service_forward.rs), leaving
  [`operond/src/main.rs`](crates/operond/src/main.rs) responsible only for gRPC auth, audit context scoping,
  and delegation for those RPCs.
- Completed hardening milestone: v0.8.10 replaced Linux FUSE mount inode-table
  write-lock panics with helper-mediated errors that callbacks return as
  errno responses.
- Completed hardening milestone: v0.8.11 replaced CLI UDP datagram forwarding
  peer-state lock panics with helper-mediated errors.
- Completed hardening milestone: v0.8.12 replaced the daemon UDP datagram
  forwarding session invariant panic with an explicit peer close response.
- Completed hardening milestone: v0.8.13 replaced daemon exec-log and Linux
  mount remote runtime invariant panics with logged or returned errors.
- Completed hardening milestone: v0.8.14 replaced the daemon onboarding token
  invariant panic with a normal CLI error.
- Completed hardening milestone: v0.8.15 replaced token generation's
  panic-style `String` formatting invariant with direct hex encoding.
- Completed model cleanup milestone: v0.8.16 removed provider from endpoint
  config, CLI output, mDNS discovery records, and SDK endpoint types.
- Completed config cleanup milestone: v0.8.17 warns about unknown config fields
  while continuing to load valid endpoint configuration.
- Completed docs/help/skills sync milestone: v0.8.18 added a validation gate
  for docs, public CLI help paths, repo-local skills, and AGENTS.md sync rules.
- Completed acceptance milestone: v0.9 endpoint model acceptance and mDNS
  discovery UX validation.
- Completed discovery UX milestone: v0.9.1 mDNS export conflict handling,
  optional discovery health checks, and endpoint-only external generator docs.
- Completed capability truthfulness milestone: v0.9.2 daemon capability
  discovery is derived from `PolicyConfig`.
- Completed runtime visibility milestone: v0.9.3 daemon audit inspection loads
  persisted audit events from the append-only store at startup while preserving
  bounded in-memory retention.
- Completed runtime hardening milestone: v0.9.4 consolidated service health
  audit semantics, store-backed exec log restart visibility, workspace
  traversal fallback validation, shell-free argv execution, config LAN
  advertisement UX, and protocol version alignment.
- Completed policy language milestone: v0.9.5 added shared policy decision
  vocabulary, stable deny reason codes, effective policy grants in
  `operon config explain`, and policy audit validation coverage.
- Completed capability diagnostics milestone: v0.9.6 added daemon-owned
  `ExplainCapability` policy diagnostics through gRPC, CLI, and TypeScript SDK.
- Completed execution model milestone: v0.10 replaced the active `job` surface
  with the unified `exec` capability across gRPC protocol, daemon runtime, CLI,
  TypeScript SDK, policy, audit, examples, docs, skills, and validation. The
  legacy job command group is intentionally not retained as a compatibility
  alias.
- Next planned milestone: define the next phase after v0.10.
- Browser management UI and CLI TUI console are no longer planned product
  surfaces.
- Network layer: outsourced to Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, Kubernetes, or manual endpoints.
- v0.1 should assume nodes are already reachable over TCP.
- Operon config should model reachable `grpc://` or `grpcs://` endpoints, not
  network providers.
- mDNS discovery is only a convenience mechanism for finding local endpoint
  candidates.
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
- Skills explain scenarios and command choice; CLI help is the source of truth
  for exact flags and arguments.
- When CLI commands, config shape, endpoint/discovery behavior, docs, or skills
  change, update README, PROTOCOL, relevant docs/plan files, repo-local skills,
  and AGENTS.md in the same task when affected.
- Run [`scripts/verify-docs-help-skills-sync.sh`](scripts/verify-docs-help-skills-sync.sh) after changes that touch CLI
  help, docs, skills, endpoint/discovery behavior, or AGENTS.md rules.
- `operon onboard` is only a guided wrapper over normal config files and CLI setup primitives; keep command-style configuration available for scripts and CI.
- `config.yaml` is the only supported runtime config format. CLI and daemon settings can be separate sections, but they should stay under the same config entrypoint with file references for sensitive values.

## First MVP Boundary

Prioritize:

- node identity
- authenticated RPC
- manually configured reachable endpoints
- capability discovery
- filesystem read/write
- command execution
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
- Public release tags, Rust crate versions, the TypeScript SDK package version,
  and `PROTOCOL_VERSION` must align for public releases so CLI `--version`,
  SDK package metadata, and runtime health output report the same release line.
- Before creating or publishing a public release tag, confirm the release
  commit is already merged to `main`; release tags must be created from the
  commit currently intended for `main`, not from an unmerged feature branch.
- Every public release must update [`scripts/verify-readme-quickstart-docker.sh`](scripts/verify-readme-quickstart-docker.sh)
  when README Quickstart, release packaging, install prerequisites, or agent
  skills guidance changes, and must run that script against the public release
  before publishing or declaring the release complete.
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
  `StreamExecLogs` now uses event envelopes, [`operon-store`](crates/operon-store) exposes an
  append-only writer boundary, daemon persistence failures surface at runtime
  boundaries, [`operon-mount`](crates/operon-mount) is a Linux FUSE adapter crate boundary, CI
  includes the v0.6.12 validation script, and the post-release documentation
  drift pass aligned current docs with the v0.6.12 runtime. Nothing remains in
  v0.6.12.
- Latest phase status update: v0.8.3 completed Read Range and Release Cleanup.
  `ReadFileRange` is implemented in proto, daemon, Linux mount, SDK, docs, and
  CI validation. Nothing remains in v0.8.3.
- Latest phase status update: v0.8.4 completed the first Runtime and CLI
  Modularization pass. Daemon fs and pagination logic plus CLI fs/output/target
  logic are extracted and validated. Exec/service/audit and non-fs CLI command
  extraction remains future maintainability work.
- Latest phase status update: v0.8.5 completed. [`operon-core`](crates/operon-core) domain modules
  are split, compatibility re-exports remain in place, CI has a dedicated
  v0.8.5 validation script, and no behavior or schema work remains in this
  phase.
- Latest phase status update: v0.8.6 completed Runtime, CLI, and Client
  Modularization. Shared Rust gRPC client helpers, non-fs CLI command modules,
  Linux mount modules, daemon auth/audit/state/exec/service internals,
  graph/workflow aliases, SDK public API alignment, `fs read --output --json`
  summary output, and low-risk validation shell helpers are complete. Nothing
  remains in v0.8.6.
- Latest phase status update: v0.8.7 completed Filesystem Service Reuse
  Cleanup. [`fs_service.rs`](crates/operond/src/fs_service.rs) now uses helper boundaries for authorization,
  workspace path resolution, and failed-audit handling. Nothing remains in
  v0.8.7.
- Latest phase status update: v0.8.8 completed Filesystem Stream Handler
  Cleanup. Full-file `ReadFile` and `WriteFile` stream behavior now lives in
  [`fs_service.rs`](crates/operond/src/fs_service.rs), and the runtime router delegates those RPCs. Nothing
  remains in v0.8.8.
- Latest phase status update: v0.8.9 completed Service Tunnel Boundary
  Cleanup. TCP and UDP service tunnel target parsing, authorization, protocol
  checks, audit handling, and connection setup now live in
  [`service_forward.rs`](crates/operond/src/service_forward.rs), and the runtime router delegates those RPCs. Nothing
  remains in v0.8.9.
- Latest phase status update: v0.8.10 completed Mount Lock Hardening.
  [`operon-mount`](crates/operon-mount) FUSE callbacks no longer panic on poisoned inode-table write
  locks; they return errno responses or propagated mount errors. Nothing
  remains in v0.8.10.
- Latest phase status update: v0.8.11 completed CLI Datagram Lock Hardening.
  UDP datagram forwarding peer-state lock failures now return errors or stop
  forwarding instead of panicking. Nothing remains in v0.8.11.
- Latest phase status update: v0.8.12 completed Daemon Datagram Invariant
  Cleanup. Missing UDP datagram peer sessions now produce an explicit peer
  close response instead of a daemon panic. Nothing remains in v0.8.12.
- Latest phase status update: v0.8.13 completed Production Panic Cleanup.
  Daemon exec-log append and Linux mount remote runtime invariant failures now
  use logged or returned errors instead of production panics. Nothing remains
  in v0.8.13.
- Latest phase status update: v0.8.14 completed Onboard Invariant Cleanup.
  Daemon onboarding token invariant failures now return a normal CLI error
  instead of panicking. Nothing remains in v0.8.14.
- Latest phase status update: v0.8.15 completed Token Generation Panic
  Cleanup. CLI token generation now uses direct hex encoding without a
  panic-style `String` formatting invariant. Nothing remains in v0.8.15.
- Latest phase status update: v0.8.16 completed Endpoint Model
  Simplification. User-facing config, CLI output, mDNS discovery records, and
  SDK endpoint types now use endpoint-only node records without provider
  metadata. Nothing remains in v0.8.16.
- Latest phase status update: v0.8.17 completed Config Unknown Field
  Warnings. Config loading now lists unknown field paths as warnings while
  continuing to load valid endpoint configuration. Nothing remains in v0.8.17.
- Latest phase status update: v0.8.18 completed Docs, Help, and Skills
  Synchronization. Docs and repo-local skills now use current endpoint-only
  discovery syntax, graph/workflow help paths are validated, CI runs the sync
  gate, and AGENTS.md records the synchronization rule. Nothing remains in
  v0.8.18.
- Latest phase status update: v0.9 completed Endpoint Model Acceptance.
  Endpoint-only example config, mDNS endpoint candidate records, endpoint-only
  discovery export, and no automatic policy grants are covered by
  [`scripts/verify-v0.9-endpoint-model.sh`](scripts/verify-v0.9-endpoint-model.sh). Nothing remains in v0.9.
- Latest phase status update: v0.9.1 completed Post-v0.9 Discovery UX.
  Discovery export now refuses same-node endpoint conflicts, optional
  `--check-health` reports endpoint health in discovery output, external
  endpoint config generator guidance is documented, and
  [`scripts/verify-post-v0.9-discovery-ux.sh`](scripts/verify-post-v0.9-discovery-ux.sh) covers the behavior. Nothing
  remains in v0.9.1.
- Latest phase status update: v0.9.2 completed Policy-Derived Capability
  Discovery. Daemon capability lists now come from `PolicyConfig`; fs, exec, and
  service capabilities are advertised only when configured, service denial audit
  ids use `service:<service_id>`, and
  [`scripts/verify-policy-derived-capabilities.sh`](scripts/verify-policy-derived-capabilities.sh) covers the behavior. Nothing
  remains in v0.9.2.
- Latest phase status update: v0.9.3 completed Store-Backed Audit Visibility.
  Daemon startup now loads persisted audit events from the append-only JSONL
  store into the bounded in-memory audit queue, and
  [`scripts/verify-v0.9.3-store-backed-audit-visibility.sh`](scripts/verify-v0.9.3-store-backed-audit-visibility.sh) covers the behavior.
  Nothing remains in v0.9.3.
- Latest phase status update: v0.9.7 completed Runtime API Hardening.
  `ListFs` now uses paginated request/response metadata, CLI/mount/SDK helpers
  preserve complete-list behavior by walking pages, SDK file writes stream
  `ReadableStream` bodies without full pre-buffering, empty daemon exec requests
  are rejected, runtime API docs list bidirectional tunnel RPCs explicitly,
  release/version surfaces were aligned to v0.9.9, and the follow-up
  documentation link audit linked source, crate, protocol, script, workflow,
  example, and skill references. Nothing remains in v0.9.7.
- Latest phase status update: v0.10 completed Execution Capability
  Unification. Active protocol RPCs/messages/enums, Rust core/daemon/CLI,
  TypeScript SDK helpers, policy and audit vocabulary, examples, docs,
  repo-local skills, and validation scripts now use `exec`. The legacy job
  command group is not retained as a supported command, and
  [`scripts/verify-v0.10-exec-unification.sh`](scripts/verify-v0.10-exec-unification.sh)
  covers the active-surface migration. Nothing remains in v0.10.
