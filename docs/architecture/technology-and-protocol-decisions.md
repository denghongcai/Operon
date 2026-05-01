# Technology and Protocol Decisions

This document records the current architecture decisions for Operon based on the early project discussion in `docs/dicussions/computer-mesh-operon.md`.

## Context

Operon is daemon-first infrastructure. Each node runs a long-lived agent that exposes local capabilities such as filesystem access, process execution, service/port access, and later screen/audio/input.

Operon should run on top of an existing secure private network instead of implementing its own network mesh. Supported network environments include Cloudflare Mesh, Tailscale, WireGuard, SSH tunnels, LAN, Kubernetes networking, and manually configured private endpoints.

The daemon must be easy to distribute across multiple operating systems and CPU architectures, including:

- x86_64
- aarch64
- armv7
- selected MIPS targets where practical

Because the daemon is the core unit of distribution, the stack should prioritize:

- single-binary installation
- low idle memory usage
- predictable cross-compilation
- service-manager friendliness
- stable streaming protocols
- clear capability contracts
- clean separation between network reachability and capability authorization

Operon should not implement:

- NAT traversal
- relay network infrastructure
- VPN/device mesh IP assignment
- global routing
- subnet routing
- packet-level network policy

## Decision 1: Rust for Core Daemon

The core daemon should be implemented in Rust.

Rust is the right default for `operond` because it gives Operon:

- single-binary distribution
- strong cross-compilation story
- low runtime overhead
- good system integration
- memory safety without a garbage-collected runtime
- direct access to filesystem, process, socket, and service APIs

This is a change from the initial TypeScript-first prototype idea. TypeScript remains useful, but not as the daemon runtime.

Recommended Rust crates:

```text
async runtime: tokio
gRPC: tonic
protobuf: prost
HTTP API: axum
CLI: clap
serialization: serde
config: figment or config
logging/tracing: tracing
TLS: rustls
identity: ed25519-dalek
filesystem watch: notify
process: tokio::process, portable-pty later
storage: rusqlite or sqlx sqlite
```

## Decision 2: TypeScript for SDK and Rust for Console UX

TypeScript should be used for surfaces that benefit from fast iteration and AI ecosystem integration:

- JavaScript/TypeScript SDK
- examples
- agent integration helpers
- documentation tooling

The TypeScript SDK should not define the core protocol independently. It should be generated from, or validated against, the shared protocol schema.

Operator console UX should be terminal-first and live inside the Rust CLI. A
separate graphical management UI is not part of the current product roadmap.

Recommended TypeScript stack:

```text
package manager: pnpm workspace
SDK gRPC client: nice-grpc
SDK protocol types: ts-proto generated protobuf types
tests: vitest
build: tsup
```

Recommended console stack:

```text
CLI: clap
TUI: ratatui or another Rust terminal UI crate
terminal events: crossterm
```

## Decision 3: gRPC for Daemon Core Protocol

Operon should use gRPC for daemon-to-daemon and CLI-to-daemon core protocol calls.

The protocol has native streaming requirements:

- file read streams
- file write streams
- process stdout/stderr streams
- job status streams
- execution event streams
- filesystem watch events
- later screen/audio streams
- possible bidirectional capability streams

gRPC handles these better than ad hoc JSON-RPC:

- first-class server streaming
- first-class client streaming
- first-class bidirectional streaming
- typed contracts through protobuf
- generated clients for multiple languages
- backpressure through HTTP/2 streaming
- standard metadata for auth/session context

The protobuf schema should be treated as the source of truth for node protocol contracts. In v0.5, that contract lives at `proto/operon/runtime.proto`, Rust bindings are generated through tonic/prost, and the TypeScript SDK uses `nice-grpc` with generated `ts-proto` types for `grpc://` endpoints.

Example shape:

```proto
service OperonNode {
  rpc Stat(StatRequest) returns (StatResponse);
  rpc ReadFile(ReadFileRequest) returns (stream FileChunk);
  rpc WriteFile(stream WriteFileChunk) returns (WriteFileResponse);

  rpc RunProcess(RunProcessRequest) returns (stream ProcessEvent);
  rpc ExecuteOperon(ExecuteRequest) returns (stream ExecutionEvent);

  rpc WatchFs(WatchFsRequest) returns (stream FsEvent);
}
```

## Decision 4: HTTP/JSON Facade for AI and Scripts

Operon should not force all consumers to speak gRPC directly.

The local daemon should expose an HTTP facade for human scripts, AI agents, and debugging:

```text
AI SDK / Scripts
          |
HTTP JSON / SSE / WebSocket
          |
      local operond
          |
   gRPC streaming protocol
          |
remote operond / worker
```

This keeps daemon internals strongly typed while preserving easy integration for:

- curl-based debugging
- LLM tool calls
- lightweight automation scripts

Recommended split:

```text
daemon core: gRPC
local control API: HTTP + JSON
event streaming to SDK/tools: SSE or WebSocket where useful
operator console: Rust CLI TUI over the runtime clients
```

## Decision 5: Workspace Layout

The repository should be organized around a Rust core and TypeScript integration layer.

Proposed layout:

```text
crates/
  operond          # daemon: identity, capability server, sessions
  operon-cli       # CLI
  operon-core      # shared execution model, policy primitives
  operon-protocol  # protobuf/gRPC contracts
  operon-fs        # filesystem capability
  operon-process   # process/job capability
  operon-store     # SQLite registry, audit, sessions
  operon-network   # endpoint resolution and provider adapters
  operon-mount     # Linux FUSE adapter

packages/
  sdk-js           # TypeScript SDK

proto/
  operon/
    node.proto
    capability.proto
    execution.proto
    policy.proto
```

## Decision 6: Network Layer Boundary

Operon should outsource connectivity to mature network layers.

The network layer answers:

```text
Can node A reach node B's agent endpoint?
```

Operon answers:

```text
Is this subject allowed to use this capability on that node?
What did the execution do?
Which logs, artifacts, and policy decisions were produced?
```

Recommended v0.1 behavior:

```text
Assume nodes are already reachable over TCP.
Use manually configured endpoints.
Do not auto-discover or provision network connectivity.
```

Example:

```yaml
nodes:
  cloud-a:
    endpoint: https://100.96.12.34:7788
  gpu-node:
    endpoint: https://100.96.18.20:7788
```

The provider abstraction should stay small:

```ts
interface NetworkProvider {
  resolveNode(nodeId: string): Promise<NodeEndpoint>
  healthCheck(nodeId: string): Promise<boolean>
}

type NodeEndpoint = {
  nodeId: string
  address: string
  port: number
  provider: "manual" | "cloudflare-mesh" | "tailscale" | "wireguard" | "ssh" | "lan" | "kubernetes"
}
```

Provider adapters should resolve or discover reachable endpoints. They should not replace Operon's policy, identity, session, or audit model.

Planned provider progression:

```text
v0.1:
  manual endpoint config
  local LAN config

v0.2:
  Cloudflare Mesh adapter
  Tailscale adapter
  SSH endpoint adapter

v0.3:
  Cloudflare API discovery
  Tailscale API discovery
  LAN mDNS
  Kubernetes service discovery
```

## Decision 7: Distribution Strategy

The daemon should ship as prebuilt binaries.

Initial target set:

```text
linux-x86_64
linux-aarch64
macos-x86_64
macos-aarch64
windows-x86_64
```

Extended target set:

```text
linux-armv7
linux-mipsel
linux-mips64el
windows-aarch64
```

MIPS and small edge devices should be treated carefully. Some dependencies, especially TLS, SQLite, FUSE, QUIC, and native service bindings, may complicate builds.

To keep distribution practical, Operon should support multiple daemon profiles:

```text
operond-full
  Full node daemon with storage, fs, process, gRPC, HTTP facade, and later mount support.

operond-lite
  Small edge daemon with core capabilities and basic RPC. No mount layer and fewer optional dependencies.
```

Release tooling candidates:

```text
cargo-dist
cross
cargo-zigbuild
GitHub Actions build matrix
```

## Decision 8: RPC Direction

For v0.1, Operon should prioritize correctness and debuggability:

```text
gRPC over HTTP/2 with rustls
local HTTP control API through axum
```

QUIC, NAT traversal, and relay work should not be part of Operon's core roadmap unless the project later proves a capability-specific need that existing network providers cannot satisfy.

Recommended progression:

```text
v0.1:
  gRPC core protocol
  local HTTP/JSON facade
  filesystem and process capability streams
  manual node endpoint config

v0.2:
  network provider adapters
  stronger auth metadata
  signed node identity
  execution trace persistence

v0.3:
  provider API discovery
  service / port access capability
  Linux FUSE mount adapter
```

## Non-goals for v0.1

Operon should avoid these in the first implementation:

- full remote desktop
- full file synchronization engine
- Kubernetes-style orchestration
- plugin marketplace
- complex policy language
- QUIC-first transport
- NAT traversal
- relay network
- VPN/device mesh IP assignment
- global routing
- browser-native gRPC requirement
- all-architecture support on day one

The first milestone should prove the core model:

```text
two or more nodes
secure identity
reachable private endpoints
capability discovery
filesystem read/write stream
process execution stream
execution trace
local CLI and SDK control
```

## Summary

Operon should be built as:

```text
Rust daemon core
+ gRPC streaming node protocol
+ HTTP/JSON local facade
+ TypeScript SDK and Rust CLI TUI console
+ network provider adapters over existing private networks
+ protobuf contracts as protocol source of truth
+ staged binary distribution across architectures
```

This gives the project a practical daemon distribution story without taking ownership of the heavy networking problems that Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, and Kubernetes already solve.

## References

- Cloudflare Mesh: https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/private-net/warp-to-warp/
- Cloudflare Private Networks: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/private-net/
