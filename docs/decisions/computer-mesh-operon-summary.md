# Computer Mesh to Operon Decision Summary

This document summarizes the product and architecture decisions from `docs/dicussions/computer-mesh-operon.md`.

## Decision

The project should be positioned as **Operon**, an AI-native capability runtime for distributed computers connected by existing private networks.

Operon should not be framed as a remote desktop, cloud computer, file sync tool, SSH wrapper, VPN, or networking mesh. It should be framed as a capability runtime where multiple reachable machines expose selected capabilities that can be discovered, authorized, composed, executed, and audited.

Core positioning:

```text
Operon is an AI-native capability runtime over your existing
private network.
```

Core model:

```text
Operon = Capability + Context + Policy + Execution
```

## Background

The original idea was a "computer mesh" network that connects local machines, cloud machines, filesystems, images, audio, and processes so one machine can operate another.

The discussion concluded that this list describes resource types, but not the system abstraction. A durable project needs a deeper model:

```text
Node
Identity
Network Provider / Endpoint
Capability
Resource URI
Session
Policy
Audit
```

The project should therefore evolve from:

```text
filesystem + image + audio + process bridge
```

to:

```text
AI-native capability runtime over existing private networks
```

This means Operon should run on top of Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, Kubernetes networking, or manually configured private endpoints.

Operon should not own:

```text
NAT traversal
relay network
VPN
device mesh IP assignment
global routing
subnet routing
```

## Core Abstractions

### Node

A node is a reachable machine, VM, container, cloud instance, local device, or sandbox running the Operon daemon.

### Capability

A capability is something a node can expose and another authorized node can invoke.

Initial capabilities:

```text
FileSystemCapability
ProcessCapability
JobCapability
ServiceCapability
SecretCapability
DeviceInfoCapability
```

Later capabilities:

```text
ScreenCapability
AudioCapability
ClipboardCapability
InputCapability
DeviceCapability
```

### Resource URI

Resources should be addressable through a unified namespace:

```text
mesh://local/fs/Users/me/project
mesh://cloud-a/fs/home/ubuntu/app
mesh://lab-gpu/process/1234
mesh://mac/screen/main
mesh://cloud-a/port/3000
```

The URI model is important because it gives agents and automation a stable way to refer to distributed resources.

### Session

A session represents a cross-node operation context. It should include identity, authorization, selected capabilities, logs, execution state, and cancellation boundaries.

### Policy

Policy defines who can access which capability, under what constraints, and with what level of permission.

### Audit

Every meaningful operation should be auditable. Operon should produce execution traces rather than opaque remote side effects.

## SecretCapability Decision

SecretCapability should exist, even if the MVP starts with a minimal version.

Reason:

```text
A node often needs another node to operate a third-party system
without exposing raw credentials as strings.
```

Examples:

- local asks cloud node to pull a private GitHub repository
- GPU node uploads results to cloud storage
- remote job needs scoped environment variables
- agent needs to call a provider API without seeing the raw token

SecretCapability is not primarily about storing secrets. It is about brokering authorized use of credentials without turning tokens into plain text passed around the mesh.

MVP shape:

```text
secret id
scope
allowed node
allowed command/job
expiry
audit log
```

Long-term meaning:

```text
SecretCapability = secure credential broker for the capability network
```

## Filesystem Decision

Filesystem access should be designed as a protocol first, not a mount layer first.

Goal:

```text
remote resources should feel local when appropriate
```

User-facing examples:

```bash
ls ~/mesh/cloud-a/home/ubuntu/project
cat ~/mesh/mac/Users/me/a.txt
echo hello > ~/mesh/gpu-node/tmp/test.txt
```

Internal model:

```text
mesh://cloud-a/fs/home/ubuntu/project
mesh://mac/fs/Users/me/a.txt
mesh://gpu-node/fs/tmp/test.txt
```

Phasing:

```text
Phase 1: explicit fs protocol calls
Phase 2: CLI convenience layer
Phase 3: local virtual mount
Phase 4: sync/cache/watch semantics
```

FUSE/WinFsp should come after the protocol is stable.

## Remote KVM / Cloud Computer Boundary

Operon is related to remote control tools, but should not be described as Remote KVM or cloud computer infrastructure.

Difference:

| Dimension | Remote KVM / Cloud Computer | Operon |
| --- | --- | --- |
| Primary unit | whole machine | capability |
| Interaction | screen/input | structured execution |
| Automation | indirect | first-class |
| Observability | weak | traceable |
| Agent friendliness | low | high |
| Security boundary | machine/session | scoped capability/policy |

Remote KVM means:

```text
operate a machine remotely
```

Operon means:

```text
compose capabilities across machines
```

This distinction is central to the project's uniqueness.

## Network Layer Boundary

Operon should outsource connectivity to mature network layers.

Recommended network providers:

```text
Cloudflare Mesh
Tailscale
WireGuard
SSH
Local LAN
Kubernetes networking
manual endpoints
```

These providers answer:

```text
Can node A reach node B's agent endpoint?
```

Operon answers:

```text
Can this subject use this capability?
What policy allowed or denied the request?
What execution graph was produced?
What logs, artifacts, and audit records exist?
```

Cloudflare Mesh, Tailscale, and similar systems can provide private IPs, encrypted connectivity, routing, and network-level access control. Operon must still provide its own capability-level authorization.

Network reachability must never imply:

```text
fs.read permission
fs.write permission
job.run permission
secret use permission
delete permission
```

The v0.1 network model should be:

```text
Assume nodes are already reachable over TCP.
Configure endpoints manually.
Do not auto-provision networking.
```

Example:

```yaml
nodes:
  cloud-a:
    endpoint: https://100.96.12.34:7788
  gpu-node:
    endpoint: https://100.96.18.20:7788
```

Later versions can add provider adapters for endpoint resolution and discovery.

## AI-native Decision

The project should lean into being AI-native / agent-native.

The competitive problem:

```text
If Operon is only remote file + remote exec + port forward,
it looks like SSH + Tailscale + rsync.
```

The differentiator:

```text
agents can discover capabilities, compose actions,
execute across nodes, and reason over structured traces.
```

AI-native requirements:

- machine-readable capability discovery
- stable resource URIs
- structured execution units
- execution graph output
- logs, status, artifacts, and errors as data
- policy boundaries clear enough for delegation
- SDKs designed for agents, not only humans

## Naming Decision

The name **Operon** should be used.

Reasoning:

In biology, an operon is a coordinated unit of gene expression. In this project, an Operon becomes a coordinated unit of cross-machine capability execution.

Project-specific meaning:

```text
Operon = a schedulable, composable, traceable distributed execution unit
```

This gives the name semantic depth instead of making it only a brand.

## MVP Scope

The MVP should prove the capability runtime, not the entire future vision.

Recommended MVP:

```text
node identity
authenticated RPC
manually configured reachable endpoints
capability discovery
filesystem read/write
process/job execution
execution trace
permission policy
CLI
minimal SDK
```

Defer:

```text
screen streaming
audio
remote desktop
clipboard sync
full mount layer
full file sync engine
complex secret manager
marketplace/plugin system
NAT traversal
relay network
VPN/device mesh IP assignment
```

## Implementation Roadmap Summary

### Phase 1: Minimal Kernel

Prove two already-reachable nodes can authenticate and perform filesystem and process operations.

### Phase 2: Filesystem Ergonomics

Add higher-level fs commands and prepare for virtual mount semantics.

### Phase 3: Process and Job Runtime

Support long-running jobs, streaming logs, cancellation, artifacts, and status queries.

### Phase 4: Policy and Audit

Make capability use explicit, scoped, and traceable.

### Phase 5: Agent-native SDK

Expose discovery, execution, and trace inspection APIs designed for AI agents.

### Phase 6: Mounts, Provider Discovery, and Advanced Capabilities

Add FUSE/WinFsp, provider API discovery, service/port access, screen/input/audio, and richer device capabilities once the core model is stable.

## Product Statement

Operon should be described as:

```text
An open-source AI-native capability runtime that runs on top of
Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, Kubernetes networking,
or any private network.

Each node can expose selected capabilities such as filesystem, process,
service ports, secrets, jobs, and devices. Authorized agents and users can
discover, invoke, compose, and audit these capabilities through structured
execution units called operons.
```

## Consequences

This decision implies:

- capability contracts are more important than UI in the early project
- execution traces are a product feature, not an implementation detail
- security and policy need to exist from the beginning, even if simple
- filesystem and process capabilities are the right first wedge
- remote desktop features should not define the project
- private networking should be treated as an external dependency
- provider adapters should resolve endpoints, not implement connectivity
- AI SDK design is part of the core product, not an afterthought

## Open Questions

- What is the exact protobuf schema for capabilities and execution events?
- How should node identity and trust establishment work in v0.1?
- How much policy expressiveness is required before OPA/Rego or another policy engine becomes useful?
- What is the first killer demo: distributed file workflow, remote job execution, or AI-run multi-node task?
- Should `mesh://` remain the URI scheme or should it become `operon://`?
- Which provider adapter should come first after manual endpoints: Cloudflare Mesh, Tailscale, SSH, or LAN discovery?

## References

- Cloudflare Mesh: https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/private-net/warp-to-warp/
- Cloudflare Private Networks: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/private-net/
