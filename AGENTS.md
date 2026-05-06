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

- `docs/plan/v0.10.1-fs-consistency-workspace-hardening.md`
  - v0.10.1 scope for filesystem versions, mutation preconditions, guarded
    CLI/SDK writes, Linux `openat2(RESOLVE_BENEATH)` workspace hardening, and
    validation.

- `docs/plan/v0.10.2-operator-diagnostics.md`
  - v0.10.2 scope for `operon doctor` config, endpoint, auth, protocol,
    capability, and service diagnostics.

- `docs/plan/v0.11-exec-session-pty-interactive.md`
  - v0.11 scope for PTY-backed `OpenExecSession`, `exec.session` policy,
    CLI/SDK session helpers, and validation.

- `docs/plan/v0.10.4-maintainability-cleanup.md`
  - v0.10.4 scope for daemon exec RPC routing, PTY/session module ownership,
    CLI exec gRPC helper boundaries, and validation.

- `docs/plan/v0.11.2-exec-session-hardening.md`
  - v0.11.2 scope for local TTY sizing, Unix resize forwarding, session stream
    drop termination, and cross-platform `portable-pty` follow-up assessment.

- `docs/plan/v0.11.3-platform-capability-matrix.md`
  - v0.11.3 scope for macOS/Windows core runtime alignment research, platform
    capability matrix documentation, CI smoke planning, and Linux-only mount
    boundary decisions.

- `docs/plan/v0.10.5-maintainability-cleanup.md`
  - v0.10.5 scope for daemon service tunnel module boundaries and CLI service
    forwarding gRPC helper boundaries.

- `docs/plan/v0.12-release-distribution-readiness.md`
  - v0.12 completed scope for Linux/macOS/Windows public release artifacts,
    packaging, checksums, smoke validation, release docs, and version
    alignment.

- `docs/plan/v0.12.1-platform-parity-hardening.md`
  - v0.12.1 completed scope for Windows private-file semantics, Windows exec
    cancellation guarantees, macOS/Windows `portable-pty` smoke coverage, and
    platform-aware doctor diagnostics.

- `docs/plan/v0.12.2-maintainability-cleanup.md`
  - v0.12.2 completed scope for behavior-preserving daemon runtime router and
    CLI exec argument/session UI cleanup.

- `docs/plan/v0.12.3-windows-exec-process-tree-cancellation.md`
  - v0.12.3 completed scope for Windows Job Object based exec process-tree
    cancellation and platform cancellation diagnostics.

- `docs/plan/v0.12.4-release-artifact-verification.md`
  - v0.12.4 completed scope for verifying public GitHub Release artifacts,
    checksums, README Quickstart alignment, and cross-platform binary smoke
    checks.

- `docs/plan/v0.12.5-cli-grpc-maintainability-cleanup.md`
  - v0.12.5 completed scope for behavior-preserving CLI gRPC helper
    modularization.

- `docs/plan/v0.13-release-publication.md`
  - Completed v0.13 scope for publishing from `main`, verifying public release
    artifacts, and validating README Quickstart against the public release.

- `docs/plan/v0.13.1-windows-pty-validation.md`
  - Completed v0.13.1 scope for turning Windows PTY validation from deferred
    status into the explicit current release decision that Windows interactive
    exec sessions are unsupported while non-interactive Windows exec remains
    supported.

- `docs/plan/v0.13.2-windows-private-file-acl.md`
  - Completed v0.13.2 scope for Windows ACL-aware token/config private-file
    validation and diagnostics.

- `docs/plan/v0.13.3-config-onboard-maintainability.md`
  - Completed v0.13.3 scope for behavior-preserving config and onboard
    plan/render/write boundary cleanup.

- `docs/plan/v0.13.4-ci-validation-consolidation.md`
  - Completed v0.13.4 scope for consolidating version-scoped CI validation
    jobs into grouped `Validation` jobs while keeping individual validation
    scripts.

- `docs/plan/v0.13.5-daemon-service-management.md`
  - Completed v0.13.5 scope for `operond service install/start/stop/status/uninstall`
    through platform-native supervision while explicitly keeping
    `operond start` foreground-only and avoiding `operond start --background`.
    Linux uses user-level systemd, macOS uses launchd user agents, and Windows
    uses a hidden `operond service run --config <path>` entrypoint that
    implements the Windows Service Control Manager protocol. Platform smoke CI
    runs daemon service-management tests on macOS and Windows.

- `docs/plan/v0.13.6-test-hardening.md`
  - Completed v0.13.6 scope for focused test hardening across Linux mount adapter
    behavior, network service checks, shared gRPC client helpers, CLI negative
    paths, RAII test cleanup, duplicate token-test cleanup, and coverage docs.

- `docs/plan/v0.13.7-mount-adapter-strategy.md`
  - Completed v0.13.7 scope for macFUSE and WinFsp mount adapter strategy,
    dependency, packaging, CI, and support-boundary decisions. Its Linux-only
    pre-v1.0 support decision is superseded by the v0.14 cross-platform live
    mount plan.

- `docs/plan/v0.13.8-mount-core-boundary.md`
  - Completed v0.13.8 scope for extracting platform-neutral `RemoteFs` and path
    behavior before attempting macFUSE FSKit or WinFsp native adapter
    implementation.

- `docs/plan/v0.14-cross-platform-live-mount.md`
  - Completed v0.14 scope for making live mount a complete core capability
    across Linux, macOS, and Windows. Shared mount-core behavior, Unix
    FUSE/FUSE-T gating, Windows WinFsp adapter code, CLI dispatch, doctor
    diagnostics, docs, validation wiring, release artifacts, and public
    Quickstart verification are complete.

- `docs/plan/v0.14-macos-live-smoke-runbook.md`
  - Runbook for macOS FUSE-T live mount release gates through FUSE-T's
    NFS-backed default path. Covers preflight, workflow dispatch, success
    evidence, and failure-log handling.

- `docs/plan/v0.14.1-mount-stabilization.md`
  - Completed v0.14.1 scope for mount adapter error classification hardening,
    including remote `ALREADY_EXISTS` to Unix `EEXIST` mapping and validation.

- `docs/plan/v0.15-windows-exec-session-parity.md`
  - Completed v0.15 scope for Windows interactive exec session support through
    `portable-pty`, bounded cross-platform PTY smoke validation, doctor/docs
    updates, and version alignment to `0.15.0` / `v0.15.0`.

- `docs/plan/v0.15.1-release-gate-hardening.md`
  - Completed v0.15.1 scope for release archive self-tests before upload,
    generic release gates, macOS FUSE-T dylib/rpath packaging checks, and
    public artifact verifier reuse.

- `docs/plan/v0.16-mount-runtime-ux-hardening.md`
  - Completed v0.16 scope for `operon doctor` mount runtime status/hints and
    platform-specific `operon mount` setup hints for Linux FUSE, macOS FUSE-T,
    and Windows WinFsp.

- `docs/plan/v0.16.1-generic-mount-release-naming.md`
  - Completed v0.16.1 scope for generic live-mount workflow/script names while
    keeping v0.14 compatibility wrappers.

- `docs/plan/v0.16.2-sdk-maintainability-cleanup.md`
  - Completed v0.16.2 scope for behavior-preserving TypeScript SDK helper and
    gRPC request-stream module extraction.

- `docs/plan/v0.16.3-daemon-mount-maintainability-cleanup.md`
  - Completed v0.16.3 scope for daemon exec command construction boundaries
    and shared CLI mount runtime diagnostics.

- `docs/plan/v0.16.4-mount-runtime-preflight-ux.md`
  - Completed v0.16.4 scope for `operon doctor --mount-runtime`, JSON
    readiness, and pre-mount runtime failure checks.

- `docs/plan/v0.16.5-release-publication.md`
  - In-progress v0.16.5 scope for publishing the v0.16 release line from
    `main`, running CI/CodeQL, live mount release gates, public artifact
    verification, and README Quickstart verification.

- `docs/plan/v0.17-release-ci-observability.md`
  - Completed v0.17 scope for CI/release observability cleanup, including
    validation-mode SDK checks, Windows-only test compilation coverage, and
    deterministic failure triage guidance.

- `docs/plan/v0.17.1-maintainability-cleanup.md`
  - Completed v0.17.1 scope for behavior-preserving SDK, Windows mount adapter,
    daemon runtime/router, and FUSE helper maintainability cleanup.

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
- Completed filesystem hardening milestone: v0.10.1 added opaque filesystem
  versions, optional mutation preconditions, guarded CLI/SDK writes, Linux
  fd-relative `openat2(RESOLVE_BENEATH)` workspace validation where available,
  and validation coverage.
- Completed operator diagnostics milestone: v0.10.2 added `operon doctor` with
  config warning, endpoint/auth, health/protocol, capability diagnostic, and
  service health reporting in human and JSON output.
- Completed exec session milestone: v0.11 added PTY-backed `OpenExecSession`,
  `exec.session` policy, CLI and SDK session helpers, docs, skills, and
  validation coverage, with versions aligned to `v0.11.0` / `0.11.0`.
- Completed maintainability cleanup milestone: v0.10.4 moved daemon exec RPC
  routing into [`exec_service.rs`](crates/operond/src/exec_service.rs),
  PTY/session runtime behavior into [`exec_session.rs`](crates/operond/src/exec_session.rs),
  and CLI exec streaming helpers into [`grpc_exec.rs`](crates/operon-cli/src/grpc_exec.rs).
- Completed exec session hardening milestone: v0.11.2 added local TTY sizing,
  Unix resize forwarding, response-stream drop termination, and documented
  `portable-pty` as the intended future macOS/Windows PTY validation
  abstraction.
- Completed maintainability cleanup milestone: v0.10.5 moved daemon TCP and
  UDP service tunnel state machines plus CLI service forwarding transport
  helpers behind focused module boundaries.
- Completed platform capability matrix milestone: v0.11.3 documented
  macOS/Windows core runtime candidate support, added platform smoke CI entries,
  kept release artifacts and mount support Linux-only, and kept interactive PTY
  direction on `portable-pty`.
- Completed mount adapter strategy milestone: v0.13.7 documented current
  macFUSE 5.2.0, FSKit, kernel-backend, Apple kext, WinFsp 2025/v2.1,
  native-vs-FUSE, service architecture, Rust binding, packaging, license, and
  CI implications. Its Linux-only pre-v1.0 support decision is superseded by
  the v0.14 cross-platform live mount plan.
- Completed mount-core boundary milestone: v0.13.8 moved platform-neutral
  `RemoteFs` and path validation into `operon-mount::mount_core`, removed the
  crate-root Linux gate, kept Linux FUSE adapter/session modules Linux-gated,
  added validation, and aligned the public release line to `0.13.8` /
  `v0.13.8`.
- Completed mount milestone: v0.14 made live mount a complete core Operon
  capability across Linux, macOS, and Windows. Shared mount-core operation
  mapping, Unix FUSE/FUSE-T gating, native Windows WinFsp adapter code through
  MIT `winfsp_wrs` / `winfsp_wrs_sys`, CLI dispatch, doctor diagnostics, docs,
  release workflow wiring, validation coverage, public release artifacts, and
  README Quickstart verification are complete.
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
- CI version validation scripts are maintained as separate `scripts/verify-*.sh`
  files but must be wired through
  [`scripts/ci/run-validations.sh`](scripts/ci/run-validations.sh), assigned to
  the narrowest existing validation group, and not added as new
  version-specific GitHub Actions matrix jobs. CI validation uses
  `OPERON_SKIP_SDK_TESTS=1` because the `TypeScript` job already runs
  `pnpm -r test`; keep individual scripts locally runnable with SDK tests
  enabled by default when that variable is unset. Add a separate workflow job
  only when the validation needs a materially different OS, permission model,
  service container, or trigger.
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
- Before creating, moving, or publishing a public release tag, run the manual
  `Cross-Platform Live Mount Smoke` GitHub Actions workflow on the exact
  release commit. Public release gates require successful macOS FUSE-T and
  Windows WinFsp live mount jobs; Linux live mount remains covered by the
  `linux-system` validation group. The generic release gate script is
  [`scripts/verify-release-gates.sh`](scripts/verify-release-gates.sh);
  [`scripts/verify-v0.14-release-gates.sh`](scripts/verify-v0.14-release-gates.sh)
  is only a compatibility wrapper.
- Every public release must update [`scripts/verify-readme-quickstart-docker.sh`](scripts/verify-readme-quickstart-docker.sh)
  when README Quickstart, release packaging, install prerequisites, or agent
  skills guidance changes. After publishing, verify release artifacts and README
  Quickstart through the manual `Verify Release Artifacts` and
  `Verify README Quickstart` GitHub Actions workflows. Do not substitute local
  script runs for release-completion evidence.
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
- Latest phase status update: v0.10.1 completed Filesystem Consistency and
  Workspace Hardening. Filesystem stat/list/write/copy responses now carry
  opaque versions, mutation requests can use preconditions for stale-write
  protection, Linux containment attempts fd-relative `openat2(RESOLVE_BENEATH)`
  validation where available, CLI/SDK guarded writes are wired, and
  [`scripts/verify-v0.10.1-fs-consistency-workspace-hardening.sh`](scripts/verify-v0.10.1-fs-consistency-workspace-hardening.sh)
  covers the behavior. Nothing remains in v0.10.1.
- Latest phase status update: v0.10.2 completed Operator Diagnostics.
  `operon doctor` now reports config unknown fields, endpoint/auth errors,
  health/protocol status, daemon-owned capability diagnostics, and service
  health in human or JSON output, and
  [`scripts/verify-v0.10.2-operator-diagnostics.sh`](scripts/verify-v0.10.2-operator-diagnostics.sh)
  covers the behavior. Nothing remains in v0.10.2.
- Latest phase status update: v0.11 completed Exec Session / PTY Interactive.
  `OpenExecSession` now carries start/input/resize requests and
  started/output/exit events, daemon sessions run through a PTY, policy can
  advertise and diagnose `exec.session`, CLI/SDK helpers are wired, and
  [`scripts/verify-v0.11-exec-session.sh`](scripts/verify-v0.11-exec-session.sh)
  covers the behavior. Nothing remains in v0.11.
- Latest phase status update: v0.10.4 completed Maintainability Cleanup.
  Daemon exec RPC routing now delegates through
  [`exec_service.rs`](crates/operond/src/exec_service.rs), PTY/session behavior
  lives in [`exec_session.rs`](crates/operond/src/exec_session.rs), CLI exec
  streaming helpers live in [`grpc_exec.rs`](crates/operon-cli/src/grpc_exec.rs),
  and [`scripts/verify-v0.10.4-maintainability-cleanup.sh`](scripts/verify-v0.10.4-maintainability-cleanup.sh)
  covers the module boundaries. Nothing remains in v0.10.4.
- Latest phase status update: v0.11.2 completed Exec Session Hardening.
  `operon exec session` now defaults to attached TTY dimensions, interactive
  Unix sessions forward resize events, dropped daemon response streams
  terminate remote sessions before terminal exit, `portable-pty` is recorded as
  the intended future macOS/Windows PTY validation abstraction, and
  [`scripts/verify-v0.11.2-exec-session-hardening.sh`](scripts/verify-v0.11.2-exec-session-hardening.sh)
  covers the behavior. Nothing remains in v0.11.2.
- Latest phase status update: v0.10.5 completed Maintainability Cleanup.
  Daemon TCP service tunnel stream behavior lives in
  [`service_tcp_forward.rs`](crates/operond/src/service_tcp_forward.rs), UDP
  datagram tunnel sessions live in
  [`service_datagram_forward.rs`](crates/operond/src/service_datagram_forward.rs), CLI service
  forwarding transport helpers live in [`grpc_service.rs`](crates/operon-cli/src/grpc_service.rs),
  and [`scripts/verify-v0.10.5-maintainability-cleanup.sh`](scripts/verify-v0.10.5-maintainability-cleanup.sh)
  covers the module boundaries. Nothing remains in v0.10.5.
- Latest phase status update: v0.11.3 completed Platform Capability Matrix and
  CI Smoke. macOS and Windows are documented as candidate core runtime
  platforms, CI has explicit platform smoke entries, release artifacts and mount
  support remain Linux-only, command-string exec/session shell defaults are
  platform-specific, and
  [`scripts/verify-v0.11.3-platform-capability-matrix.sh`](scripts/verify-v0.11.3-platform-capability-matrix.sh)
  covers the docs, CI, and shell-default evidence. Nothing remains in v0.11.3.
- Latest phase status update: v0.12 completed Release / Distribution
  Readiness. Draft release automation now builds Linux `x86_64`/`arm64`/`armv7`
  archives plus macOS `x86_64`/`aarch64` and Windows `x86_64` core runtime
  preview archives, smoke-tests native daemon/CLI binaries, generates
  checksums, keeps mount and GLIBC validation Linux-only, updates README and
  architecture docs, and aligns versions to `0.12.2` / `v0.12.2`.
- Latest phase status update: v0.12.1 completed Platform Parity Hardening.
  `operon doctor` reports platform caveats. Windows private-file handling was
  initially warning-only in this phase and is superseded by v0.13.2 ACL-aware
  validation; Unix-like `portable-pty` smoke coverage is in CI, and Windows PTY
  validation is reported as deferred. Windows exec cancellation was initially
  documented as direct-child best-effort in this phase and is superseded by the
  v0.12.3 Job Object work; macFUSE/WinFsp remain deferred.
- Latest phase status update: v0.12.2 completed Maintainability Cleanup.
  Daemon gRPC runtime routing now lives in [`runtime.rs`](crates/operond/src/runtime.rs),
  CLI exec shell/argv helpers live in
  [`exec_args.rs`](crates/operon-cli/src/commands/exec_args.rs), CLI PTY session UI lives in
  [`exec_session.rs`](crates/operon-cli/src/commands/exec_session.rs), and
  [`scripts/verify-v0.12.2-maintainability-cleanup.sh`](scripts/verify-v0.12.2-maintainability-cleanup.sh)
  covers the module boundaries. Nothing remains in v0.12.2.
- Latest phase status update: v0.12.3 completed Windows Exec Process-Tree
  Cancellation. Windows non-interactive exec cancellation now assigns spawned
  processes to a Job Object and terminates the process tree through
  `TerminateJobObject`, while Unix process-group behavior remains unchanged.
  CI includes Windows compile coverage and a Windows-only descendant-process
  cancellation smoke test. Nothing remains in v0.12.3.
- Latest phase status update: v0.12.4 completed Release Artifact Verification.
  [`scripts/verify-release-artifacts.sh`](scripts/verify-release-artifacts.sh)
  downloads public GitHub Release assets for a tag, verifies `SHA256SUMS`,
  checks the expected Linux/macOS/Windows/SDK asset set, and smoke-tests the
  current platform archive. The manual `Verify Release Artifacts` workflow runs
  the same verifier on Linux, macOS, and Windows. Nothing remains in v0.12.4.
- Latest phase status update: v0.12.5 completed CLI gRPC Maintainability
  Cleanup. CLI gRPC filesystem, non-session exec, service list/check, and audit
  helpers now live in focused modules, while
  [`grpc.rs`](crates/operon-cli/src/grpc.rs) remains the compatibility and
  shared connection/context surface. Nothing remains in v0.12.5.
- Latest phase status update: v0.13.4 completed CI Validation Consolidation.
  Version-scoped validation scripts remain separate, but CI runs them through
  [`scripts/ci/run-validations.sh`](scripts/ci/run-validations.sh) from grouped
  `Validation` jobs with per-script logs and a final failure summary. CI skips
  duplicate SDK unit tests after the `TypeScript` job has run `pnpm -r test`,
  while local validation scripts keep SDK tests enabled by default. Future
  version validation additions must extend that runner and choose an existing
  group unless they require a distinct OS, permission model, service container,
  or trigger.
- Latest phase status update: v0.13.5 completed Daemon Service Management.
  `operond service install/start/stop/status/uninstall` now manages
  platform-native supervisor entries for Linux user-level systemd and macOS
  launchd user agents. `operond start` remains foreground-only with no
  `--background` flag. Windows service management registers
  `operond service run --config <path>`, which implements the Service Control
  Manager protocol and maps SCM stop/shutdown controls to daemon shutdown.
  Platform smoke CI covers the daemon service-management tests on macOS and
  Windows. Nothing remains in v0.13.5.
- Latest phase status update: v0.13.6 completed Test Hardening.
  `operon-network` now has deterministic TCP/UDP service-check tests and TCP
  success reports TCP reachability. `operon-grpc-client` has chunk-boundary,
  metadata, and connection-deadline coverage, and the Linux mount remote client
  uses the same deadline helper. `operon-mount` has focused errno/FUSE helper
  tests, CLI compiled-binary integration has negative-path coverage, targeted
  tests use `TempDir` cleanup, duplicate onboard token-generation coverage was
  replaced with onboard-specific token-file/config-reference coverage, and
  [`scripts/verify-v0.13.6-test-hardening.sh`](scripts/verify-v0.13.6-test-hardening.sh)
  is wired into consolidated validation. Nothing remains in v0.13.6.
- Latest phase status update: v0.13.1 completed Windows PTY Validation.
  This phase originally chose an explicit unsupported decision for Windows
  interactive `OpenExecSession`; that decision is superseded by v0.15 Windows
  Exec Session Parity. Windows non-interactive exec and Windows Job Object
  cancellation remain in place, and
  [`scripts/verify-v0.13.1-windows-pty-validation.sh`](scripts/verify-v0.13.1-windows-pty-validation.sh)
  now validates that the historical ambiguity was closed and that current docs
  point at the supported v0.15 status. Nothing remains in v0.13.1.
- Latest phase status update: v0.13 completed Release Publication and Public
  Verification. Public GitHub Release
  [`v0.13.1`](https://github.com/denghongcai/Operon/releases/tag/v0.13.1)
  was published from `main` commit
  `e41309015f9765ea0a3ebd54dc539940c6ef9af9` after `CI` and `CodeQL` passed.
  The `Draft Release` workflow produced Linux, macOS, Windows, TypeScript SDK,
  and checksum assets; `Verify Release Artifacts` passed on Linux, macOS, and
  Windows; and README Quickstart release validation passed against the public
  tag. Nothing remains in v0.13.
- Latest phase status update: v0.13.2 completed Windows Private File ACL
  Enforcement. Windows private files generated by CLI initialization and
  onboarding are checked with an ACL model that allows only the current user,
  Administrators, and SYSTEM; broad existing ACLs are rejected, new files get a
  protected ACL, and `operon doctor` reports `windows-acl-verified`. CI has
  Windows-only private-file ACL write smoke coverage and
  [`scripts/verify-v0.13.2-windows-private-file-acl.sh`](scripts/verify-v0.13.2-windows-private-file-acl.sh)
  is wired into consolidated validation. Nothing remains in v0.13.2.
- Latest phase status update: v0.13.3 completed Config and Onboard
  Maintainability Cleanup. `operon config explain` execution/rendering now
  lives under
  [`commands/config/explain.rs`](crates/operon-cli/src/commands/config/explain.rs);
  onboarding now has explicit plan, render, and write module boundaries under
  [`onboard/`](crates/operon-cli/src/onboard). Generated config shape,
  token-file behavior, text output, and JSON summary behavior remain covered by
  focused tests, and
  [`scripts/verify-v0.13.3-config-onboard-maintainability.sh`](scripts/verify-v0.13.3-config-onboard-maintainability.sh)
  is wired into consolidated validation. Nothing remains in v0.13.3.
- Latest phase status update: v0.14 Cross-Platform Live Mount is completed.
  [`operon-mount`](crates/operon-mount) has shared `MountAdapterCore`
  operation mapping and error classification, Linux/macOS FUSE adapter gating
  through `fuser`, and a Windows native WinFsp adapter using MIT
  `winfsp_wrs` / `winfsp_wrs_sys`. `operon mount` dispatches to Linux FUSE,
  macOS FUSE-T, or Windows WinFsp builds; `operon doctor` reports platform
  runtime requirements. Final release commit `dffa1c5` passed main CI
  `25383140244`, CodeQL `25383139508`, hosted macOS FUSE-T live smoke
  `25383149119`, and Windows WinFsp live smoke `25383149153`. Public GitHub
  Release `v0.14.0` is published with Linux, macOS, Windows, TypeScript SDK,
  and checksum assets. For future public releases, run the manual
  `Cross-Platform Live Mount Smoke` workflow on the exact release commit before tagging
  or publishing; release gates require successful macOS FUSE-T and Windows
  WinFsp live mount jobs. After publishing, run the manual
  `Verify Release Artifacts` and `Verify README Quickstart` workflows for the
  public tag. Nothing remains in v0.14.
- Latest phase status update: v0.14.1 Mount Stabilization is completed.
  [`operon-mount`](crates/operon-mount) classifies remote
  `tonic::Code::AlreadyExists` separately and maps it to Unix FUSE `EEXIST`
  for existing-path create or mkdir collisions. Focused mount-core and errno
  tests plus
  [`scripts/verify-v0.14.1-mount-stabilization.sh`](scripts/verify-v0.14.1-mount-stabilization.sh)
  cover the behavior. Nothing remains in v0.14.1.
- Latest phase status update: v0.15 Windows Exec Session Parity is completed.
  Windows interactive exec sessions now use the existing `portable-pty`
  backend instead of returning `UNIMPLEMENTED`; `operon doctor` reports
  `windows-portable-pty-smoke-validated`; the platform smoke workflow runs
  bounded portable-pty smoke coverage on Windows; daemon session runtime now
  releases the portable-pty slave handle after spawn, shares PTY writer
  ownership so Windows ConPTY cursor-position queries receive a minimal
  terminal response, and smoke coverage drives a real interactive shell
  through the PTY writer; docs and validation scripts are aligned with the
  supported status and repository versions are aligned to `0.15.0` /
  `v0.15.0`. `v0.15.0` was published from `main` commit
  `f9cc8d187960f69835ea349d7ca0e4b7264d5976` after main CI `25391744639`,
  CodeQL `25391744590`, macOS FUSE-T live mount gate `25391757529`, and
  Windows WinFsp live mount gate `25391757596` passed. Release draft workflow
  `25392667228` rebuilt the public assets after macOS packaging was hardened
  to bundle `libfuse-t.dylib` with an `@executable_path` rpath, and public
  verification passed in release artifact workflow `25392962032` and README
  Quickstart workflow `25392962190`. Nothing remains in v0.15.
- Latest phase status update: v0.15.1 Release Gate Hardening is completed.
  Release archives are now extracted and smoke-tested inside the draft release
  workflow before upload; macOS archive smoke clears `DYLD_*`, requires bundled
  `libfuse-t.dylib`, and verifies the packaged `operon` binary has an
  `@executable_path` rpath. Public release artifact verification reuses the
  same archive smoke script. The release-only live mount gate is now the
  cross-version `Cross-Platform Live Mount Smoke` workflow, with old v0.14
  workflow evidence accepted only for compatibility. Nothing remains in
  v0.15.1.
- Latest phase status update: v0.16 Mount Runtime UX Hardening is completed.
  `operon doctor` now reports `mount_runtime` and `mount_hint` in human and
  JSON output, with Linux FUSE, macOS FUSE-T, and Windows WinFsp runtime
  detection. `operon mount` appends platform-specific setup hints when adapter
  startup fails, and README/PROTOCOL troubleshooting language is aligned.
  Nothing remains in v0.16.
- Latest phase status update: v0.16.1 Generic Mount and Release Naming Cleanup
  is completed. The active live-mount workflow is
  `.github/workflows/live-mount-smoke.yml`, generic macOS/Windows smoke helper
  names are available, and old `scripts/*v0.14*` names are compatibility
  wrappers. Nothing remains in v0.16.1.
- Latest phase status update: v0.16.2 SDK Maintainability Cleanup is
  completed. `packages/sdk-js/src/transport.ts` and
  `packages/sdk-js/src/grpc-requests.ts` now own SDK transport and request
  stream helpers while `index.ts` remains the public API entrypoint. Nothing
  remains in v0.16.2.
- Latest phase status update: v0.16.3 Daemon and Mount Maintainability Cleanup
  is completed. `crates/operond/src/exec_command.rs` owns daemon exec command
  construction and `crates/operon-cli/src/commands/mount_runtime.rs` owns
  shared mount runtime diagnostics. Nothing remains in v0.16.3.
- Latest phase status update: v0.16.4 Mount Runtime Preflight UX is completed.
  `operon doctor --mount-runtime` reports local mount runtime readiness without
  loading config, doctor JSON includes `mount_runtime_ready`, and `operon mount`
  fails early when the local runtime is missing. Nothing remains in v0.16.4.
- Latest phase status update: v0.17 Release and CI Observability Cleanup is
  completed. `docs/quality/release-ci-observability.md`,
  `scripts/verify-v0.17-release-ci-observability.sh`, and
  `scripts/verify-readme-quickstart-docker.sh --dry-run` now cover CI-mode SDK
  validation, Windows target daemon test compilation, release artifact dry-run
  wiring, README Quickstart dry-run wiring, deterministic workflow failure
  triage, and legacy validation drift after later file-boundary refactors.
  Nothing remains in v0.17.
- Latest phase status update: v0.17.1 Maintainability Cleanup is completed.
  SDK gRPC mapper helpers live in `packages/sdk-js/src/grpc-mappers.ts`,
  Windows mount adapter helpers live in focused `windows_*` modules, daemon CLI
  shape lives in `crates/operond/src/daemon_cli.rs`, and
  `scripts/verify-v0.17.1-maintainability-cleanup.sh` validates the moved
  boundaries. Nothing remains in v0.17.1.
- Latest phase status update: v0.16.5 Release Publication and Public
  Verification is completed. `v0.16.5` was published from `main` commit
  `c9b0737049db95d7cf241f61d73c0b26d687db28` after main CI `25442815597`,
  CodeQL `25442814311`, macOS FUSE-T live mount gate `25443368771`, and
  Windows WinFsp live mount gate `25443370961` passed. Draft release workflow
  `25443548403` built and archive-smoked the artifacts, public artifact
  verification passed in workflow `25443988140`, and README Quickstart
  verification passed in workflow `25443990441`. Nothing remains in v0.16.5.
- Next planned phase order: choose the next post-release phase after v0.16.5,
  v0.17, and v0.17.1 completion.
