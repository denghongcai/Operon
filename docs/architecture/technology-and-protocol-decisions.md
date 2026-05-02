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
minimum Rust: 1.85
async runtime: tokio
gRPC: tonic
protobuf: prost
CLI: clap
serialization: serde
config: figment or config
logging/tracing: tracing
TLS: rustls
identity: ed25519-dalek
filesystem watch: notify
Linux FUSE mount: fuser
process: tokio::process, portable-pty later
storage: rusqlite or sqlx sqlite
```

## Decision 2: TypeScript for SDK and Rust for CLI UX

TypeScript should be used for surfaces that benefit from fast iteration and AI ecosystem integration:

- JavaScript/TypeScript SDK
- examples
- repo-local agent skills and usage guidance in `skills/*/SKILL.md`
- documentation tooling

The TypeScript SDK should not define the core protocol independently. It should be generated from, or validated against, the shared protocol schema.

Operator UX should remain command-oriented through the Rust CLI. A separate
graphical management UI and CLI TUI console are not part of the current product
roadmap.

Recommended TypeScript stack:

```text
package manager: pnpm workspace
SDK gRPC client: nice-grpc
SDK protocol types: ts-proto generated protobuf types
tests: vitest
build: tsup
```

Recommended CLI stack:

```text
CLI: clap
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

The protobuf schema should be treated as the source of truth for node protocol
contracts. As of v0.6.12, the active contract lives at
`proto/operon/runtime.proto`, Rust bindings are generated through tonic/prost,
and the TypeScript SDK uses `nice-grpc` with generated `ts-proto` types for
`grpc://` endpoints. Legacy design proto files live under `proto/archive/` and
are not compiled into the runtime API.

Example shape:

```proto
service OperonRuntime {
  rpc ReadFile(FsPathRequest) returns (stream FileChunk);
  rpc WriteFile(stream WriteFileRequest) returns (FsWrite);
  rpc RunJob(JobRunRequest) returns (JobRecord);
  rpc WatchJob(JobIdRequest) returns (stream JobEvent);
  rpc StreamJobLogs(JobIdRequest) returns (stream JobLogStreamEvent);
  rpc OpenServiceTunnel(stream ServiceTunnelRequest) returns (stream ServiceTunnelResponse);
}
```

## Decision 4: CLI and SDK Instead of HTTP Runtime Facade

Operon should not keep a parallel HTTP runtime API once gRPC is available.

The supported interfaces should be:

```text
Humans / ops / scripts -> operon CLI, including --json
Programs / agents      -> SDKs generated from the gRPC protocol
Daemon runtime         -> gRPC streaming protocol
```

Service forwarding should use the same boundary. The CLI or SDK opens a local
listener, then uses `OpenServiceTunnel` to stream bytes to a policy-configured
service on the remote node. The daemon must only connect to services declared in
`policy.service.services`; clients do not send arbitrary host/port targets.
Service entries carry explicit action permissions for health checks and
forwarding.

This keeps the daemon surface smaller and avoids maintaining two runtime API
contracts. Direct HTTP runtime calls would duplicate:

- auth behavior
- structured error semantics
- streaming file transfer
- job stdin/log streaming
- documentation and validation matrices

Recommended split:

```text
daemon core: gRPC
script/control interface: operon CLI with --json
programmatic interface: TypeScript SDK with nice-grpc
direct protocol interface: generated gRPC clients from proto/operon/runtime.proto
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
  operon-mount     # OS mount adapters over RemoteFs

packages/
  sdk-js           # TypeScript SDK

proto/
  operon/
    runtime.proto
  archive/
    operon/
      node.proto
      capability.proto
      execution.proto
      policy.proto
```

## Decision 6: Mount Layer Boundary

Mount support is an adapter over the Core FS Protocol. FUSE, macFUSE, and WinFsp
should adapt OS filesystem calls into Operon filesystem operations; they should
not become the core VFS model. The current implementation provides a Linux FUSE
adapter; macFUSE and WinFsp remain future adapter work.

Current shape:

```text
OS Mount Adapter
  FUSE on Linux
      │
      ▼
Core FS Protocol
  RemoteFs trait
      │
      ▼
Runtime transport implementation
  GrpcRemoteFs
      │
      ▼
operond fs capability
  StatFs / ListFs / ReadFile / WriteFile
  WriteFileRange / TruncateFs / MkdirFs / DeleteFs / RenameFs / CopyFs
  policy / audit owned by daemon
```

The current implementation intentionally does not include local IPC between a
mount adapter and a local daemon. The adapter can call the runtime gRPC endpoint
directly through `GrpcRemoteFs`. A later local IPC implementation can be added as
another `RemoteFs` implementation without changing FUSE semantics.

Non-goals for the mount adapter layer:

- independent VFS authorization model.
- independent policy or audit decisions.
- VPN, mesh, or routing behavior.
- durable metadata store in the adapter.
- offline sync or conflict resolution in v0.6.1.

The FUSE adapter may keep transient inode mappings and rely on the kernel page
cache, but persistent metadata, sync, and richer write semantics belong in later
explicit phases.

### Current Filesystem Concurrency Contract

The current Core FS Protocol is single-writer oriented. It intentionally does
not define multi-writer conflict detection yet.

Current behavior:

- reads are live remote reads, not snapshot reads.
- writes are applied as daemon RPCs against the underlying filesystem.
- audit records the operations that were allowed or denied, but it does not
  detect semantic conflicts.
- concurrent writes to the same path are not coordinated by Operon.

Not currently present:

- file versions or etags on `FsStat`.
- expected-version preconditions on writes.
- compare-and-swap semantics.
- advisory or mandatory file locks.
- file leases.
- distributed transactions or merge/conflict resolution.

A later concurrency phase should decide whether the project needs optimistic
version checks, explicit file leases, or both. Until then, callers that require
deterministic results should serialize writes at the workflow or agent layer.

`CopyFs` is a daemon-side same-node convenience operation in the Core FS
Protocol. It exists for CLI, SDK, and direct protocol clients, not because POSIX
mount adapters receive a copy callback. Cross-node copy remains a separate
future design item because it needs source streaming, target writing, partial
failure semantics, and audit ownership across two nodes.

## Decision 7: Network Layer Boundary

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
    endpoint: grpc://100.96.12.34:7789
  gpu-node:
    endpoint: grpc://100.96.18.20:7789
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

## Decision 8: Distribution Strategy

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
  Full node daemon with storage, fs, process, gRPC, and later mount support.

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

## Decision 9: RPC Direction

For v0.1, Operon should prioritize correctness and debuggability:

```text
gRPC over HTTP/2 with rustls
operon CLI with --json for local control and scripts
```

QUIC, NAT traversal, and relay work should not be part of Operon's core roadmap unless the project later proves a capability-specific need that existing network providers cannot satisfy.

Recommended progression:

```text
v0.5:
  gRPC core protocol
  operon CLI control surface
  filesystem and process capability streams
  manual node endpoint config

v0.6:
  Linux read-only FUSE mount adapter

v0.6.1:
  Linux write FUSE mount adapter

v0.7:
  service metadata, health checks, and explicit local forwarding

v0.7.1:
  UDP/datagram service forwarding as a separate datagram protocol

v0.8:
  agent skills pack

v0.9:
  non-LAN provider API discovery
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
+ TypeScript SDK and Rust CLI
+ network provider adapters over existing private networks
+ protobuf contracts as protocol source of truth
+ staged binary distribution across architectures
```

This gives the project a practical daemon distribution story without taking ownership of the heavy networking problems that Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, and Kubernetes already solve.

## References

- Cloudflare Mesh: https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/private-net/warp-to-warp/
- Cloudflare Private Networks: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/private-net/
