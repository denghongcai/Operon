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

- `README.md`
  - Public-facing project positioning and high-level architecture.
  - Current framing: "AI-native capability runtime over existing private networks."

- `docs/decisions/computer-mesh-operon-summary.md`
  - Product and concept decision summary.
  - Covers the shift from "computer mesh" to Operon, core abstractions, AI-native positioning, network boundary, MVP scope, and open questions.

- `docs/architecture/technology-and-protocol-decisions.md`
  - Technical architecture decisions.
  - Covers Rust daemon core, TypeScript SDK/web layer, gRPC streaming protocol, HTTP/JSON facade, provider adapters, distribution targets, and non-goals.

- `docs/dicussions/computer-mesh-operon.md`
  - Raw archived discussion that led to the current direction.
  - Keep the existing folder spelling unless the repo intentionally migrates it.

## Current Architecture Decisions

- Core daemon: Rust.
- SDK and web console: TypeScript.
- Core daemon protocol: gRPC with streaming.
- Local control/API facade: HTTP + JSON, SSE, or WebSocket.
- Network layer: outsourced to Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, Kubernetes, or manual endpoints.
- v0.1 should assume nodes are already reachable over TCP.
- Provider adapters should resolve/discover endpoints, not implement connectivity.
- Capability authorization must remain inside Operon even when network access is already allowed.

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
