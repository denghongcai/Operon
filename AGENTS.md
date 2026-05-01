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

- `docs/plan/v0.7-acceptance.md`
  - v0.7 acceptance scope for the CLI TUI console.

- `docs/plan/v0.8-acceptance.md`
  - v0.8 acceptance scope for Agent Integration.

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
  - Covers Rust daemon core, TypeScript SDK, CLI TUI console direction, gRPC streaming protocol, CLI/SDK interfaces, provider adapters, distribution targets, and non-goals.

- `docs/dicussions/computer-mesh-operon.md`
  - Raw archived discussion that led to the current direction.
  - Keep the existing folder spelling unless the repo intentionally migrates it.

## Current Architecture Decisions

- Core daemon: Rust.
- SDK: TypeScript.
- CLI and CLI TUI console: Rust.
- Current core daemon protocol: gRPC with streaming.
- Human, ops, and script interface: `operon` CLI, including `--json`.
- TypeScript SDK interface: `nice-grpc`.
- HTTP runtime facade was removed in v0.5.1; do not reintroduce direct HTTP runtime APIs.
- Completed protocol milestone: v0.5 gRPC runtime protocol migration.
- Completed cleanup milestone: v0.5.1 removed the HTTP runtime facade and added `PROTOCOL.md`.
- Completed mount milestone: v0.6 Linux-only read-only real FUSE mount.
- Completed mount milestone: v0.6.1 Linux-only write-through FUSE mount.
- Next planned milestone: v0.7 CLI TUI console.
- Later planned milestones: v0.8 Agent Integration, v0.9 non-LAN provider discovery.
- Browser management UI is no longer a planned product surface; use CLI TUI console instead.
- Network layer: outsourced to Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, Kubernetes, or manual endpoints.
- v0.1 should assume nodes are already reachable over TCP.
- Provider adapters should resolve/discover endpoints, not implement connectivity.
- Capability authorization must remain inside Operon even when network access is already allowed.
- Service / port capability is metadata and TCP health checking only; it must not become port forwarding, proxying, VPN, or relay behavior.

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
- service metadata and health checks
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
- port forwarding and proxying
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
