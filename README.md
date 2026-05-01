# Operon

> The missing execution model for AI agents.

Operon is an AI-native capability runtime for machines already connected by Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, or any private network.

Instead of building another VPN or mesh network, Operon focuses on what happens after machines can reach each other: capability discovery, policy, execution graphs, audit, and agent-friendly tooling.

---

## ✨ What is an Operon?

Inspired by the concept of operons in biology, an **Operon** is a unit of coordinated execution.

In Operon:

- A **node** is any reachable machine (local, cloud, container)
- A **capability** is something a node can do (filesystem, process, service access, etc.)
- An **operon** is a composition of capabilities executed across nodes

```text
Operon = Capability + Context + Policy + Execution
```

---

## 🧠 Why Operon?

Today, AI agents interact with the real world through fragmented tools:

- SSH
- APIs
- File uploads
- Remote desktops
- VPN-connected machines

This leads to:

- ❌ Poor composability
- ❌ No execution trace
- ❌ Weak security boundaries
- ❌ Hard to automate reliably
- ❌ Network access mistaken for capability access

Operon fixes this by introducing:

- ✅ A unified capability model
- ✅ Structured execution (not just commands)
- ✅ Built-in observability
- ✅ Secure, policy-driven execution
- ✅ A clear boundary between private networking and capability authorization

Operon is not a VPN. It runs on top of Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, or any private network, and turns connected machines into a secure, AI-operable capability runtime.

---

## Quickstart

Prerequisites:

- Rust stable toolchain
- Node.js and pnpm
- Docker with Docker Compose

Run the full MVP validation:

```bash
pnpm install --frozen-lockfile
cargo fmt --check
cargo check --workspace --locked
cargo clippy --workspace --locked -- -D warnings
pnpm typecheck
scripts/verify-mvp-docker.sh
```

The Docker validation starts two reachable `operond` nodes, exercises capabilities through the CLI, checks policy and audit behavior, and runs the example execution graph.

---

## CLI and Configuration

The MVP has two binaries:

- `operond`: the daemon that runs on each reachable machine.
- `operon`: the CLI that talks to daemon endpoints.

From the repo, run them through Cargo:

```bash
cargo run -p operond -- start --listen 0.0.0.0:7788 --node-id local --workspace /workspace
cargo run -p operon-cli -- --config examples/nodes.yaml node list
```

After installing built binaries, the same commands are:

```bash
operond start --listen 0.0.0.0:7788 --node-id local --workspace /workspace
operon --config examples/nodes.yaml node list
```

### Node Config

The CLI reads node endpoints from a YAML file. In the current MVP, the default path is:

```text
examples/nodes.yaml
```

That default is useful for local development from the repo root. For real use, keep your own config file anywhere you want and pass it explicitly with `--config`:

```bash
operon --config ./operon.nodes.yaml node list
operon --config ./operon.nodes.yaml node ping cloud-a
operon --config ./operon.nodes.yaml capability list cloud-a
```

Config shape:

```yaml
nodes:
  local:
    endpoint: http://127.0.0.1:7788
  cloud-a:
    endpoint: http://100.96.12.34:7788
    provider: tailscale
  gpu-node:
    endpoint: http://100.96.18.20:7788
    provider: cloudflare-mesh
```

`provider` is optional and defaults to `manual`. In v0.1 it is metadata only; Operon does not create VPNs, assign mesh IPs, or discover nodes automatically.

Supported provider values:

```text
manual
cloudflare-mesh
tailscale
wireguard
ssh
lan
kubernetes
```

### Daemon Policy Config

`operond` accepts a local policy file:

```bash
operond start \
  --listen 0.0.0.0:7788 \
  --node-id cloud-a \
  --workspace /home/ubuntu/workspace \
  --policy ./operon.policy.yaml
```

If `--policy` is omitted, the daemon uses a permissive MVP default for the configured workspace. For any real machine, pass an explicit policy file.

Policy shape:

```yaml
subject: local-cli

fs:
  mounts:
    - name: workspace
      path: /
      permissions:
        read: true
        write: true
        delete: false

job:
  allowed_cwds:
    - /
  default_timeout_secs: 30
  max_timeout_secs: 300
  env_allowlist: []
```

Policy paths are virtual paths inside the daemon workspace. If the daemon starts with `--workspace /home/ubuntu/workspace`, policy path `/` means that workspace root, not the host root.

### Common Commands

```bash
operon --config ./operon.nodes.yaml node list
operon --config ./operon.nodes.yaml node ping cloud-a
operon --config ./operon.nodes.yaml capability list cloud-a

operon --config ./operon.nodes.yaml fs stat cloud-a:/README.md
operon --config ./operon.nodes.yaml fs list cloud-a:/
operon --config ./operon.nodes.yaml fs read cloud-a:/input.txt
operon --config ./operon.nodes.yaml fs write cloud-a:/input.txt --content "hello"

operon --config ./operon.nodes.yaml job run cloud-a -- echo hello
operon --config ./operon.nodes.yaml job run cloud-a --detach -- sleep 10
operon --config ./operon.nodes.yaml job status cloud-a job-1
operon --config ./operon.nodes.yaml job logs cloud-a job-1
operon --config ./operon.nodes.yaml job cancel cloud-a job-1

operon --config ./operon.nodes.yaml audit list cloud-a
operon --config ./operon.nodes.yaml run examples/train-model.yaml
```

---

## 🌐 Network Boundary

Operon assumes your nodes can already reach each other.

Use:

- Cloudflare Mesh
- Tailscale
- WireGuard
- SSH tunnels
- Local LAN
- Kubernetes networking

Then point Operon at reachable daemon endpoints:

```yaml
nodes:
  cloud-a:
    endpoint: https://100.96.12.34:7788
  gpu-node:
    endpoint: https://100.96.18.20:7788
```

Cloudflare Mesh or Tailscale can decide whether one device can reach another device. Operon decides whether that device can read a directory, run a job, use a secret, or inspect an execution trace.

---

## ⚡ Example

Run the local Docker MVP demo:

```bash
scripts/verify-mvp-docker.sh
```

This starts two `operond` containers, validates capability discovery, fs operations, job execution, policy denial, audit output, and runs:

```bash
operon --config examples/docker-nodes.yaml run examples/docker-copy-and-run.yaml
```

Example workflow:

```yaml
name: docker-copy-and-run

steps:
  - id: write-input
    node: node-a
    action: fs.write
    path: /graph-input.txt
    content: hello from graph

  - id: run-command
    node: node-a
    action: job.run
    cwd: /
    timeout_secs: 5
    command: cat graph-input.txt > graph-output.txt

  - id: read-output
    node: node-a
    action: fs.read
    path: /graph-output.txt
```

For real machines, point the CLI at already reachable daemon endpoints:

```bash
operon --config examples/nodes.yaml run examples/train-model.yaml
```

---

## 🔍 Execution Graph

Every operon produces a full execution trace:

```text
local
 ├── fs.read (nas)
 ├── job.run (gpu-node)
 └── fs.write (cloud-a)
```

Each step includes:

- input / output
- logs
- duration
- status

---

## 🧩 Capabilities

Operon exposes machines through capabilities:

```text
mesh://cloud-a/fs/workspace
mesh://gpu-node/job/run
mesh://mac/screen/main
```

Supported (initial):

- filesystem (read / write / list)
- job execution
- process control
- service / port access over an existing private network

Planned:

- screen / input
- audio
- clipboard
- device access

---

## 🔐 Secure by Design

Operon enforces capability boundaries:

- Nodes only expose **explicit mounts**
- Secrets are **never directly accessible**
- Execution is **policy-controlled**
- Every action is **auditable**
- Network reachability never implies capability permission

---

## 🤖 AI-native

Operon is designed for agents, not humans.

Agents can:

- discover available capabilities
- compose operons dynamically
- execute across multiple nodes
- reason over execution results

```ts
import { OperonClient } from "@operon/sdk";

const operon = new OperonClient([
  { nodeId: "cloud-a", endpoint: "http://100.96.12.34:7788" },
  { nodeId: "gpu-node", endpoint: "http://100.96.18.20:7788" }
]);

const trace = await operon.run({
  name: "train-model",
  steps: [
    { node: "cloud-a", action: "fs.read", path: "/data" },
    { node: "gpu-node", action: "job.run", command: "train.py" }
  ]
});
```

---

## 🏗 Architecture

```text
AI Agent / CLI / SDK
        ↓
Operon Runtime
  - Operon Graph
  - Scheduler
  - Execution Trace
        ↓
Capability Layer
  - fs / job / process / port / secret
        ↓
Policy / Secret / Audit
        ↓
Agent API
  - HTTP / WebSocket / gRPC
        ↓
Network Provider Adapter
  - Cloudflare Mesh / Tailscale / WireGuard / SSH / LAN
        ↓
Existing Secure Private Network
```

---

## 🚧 Status

Operon is in early development.

Roadmap:

- [x] Node runtime
- [x] Filesystem capability
- [x] Job execution
- [x] CLI
- [x] Execution graph
- [x] Minimal policy and audit
- [x] Minimal TypeScript SDK
- [ ] FUSE mount
- [ ] Agent integration
- [ ] Network provider adapters

---

## 💡 Vision

Operon is not a VPN or remote control tool.

It is a new execution model where:

> Computers are no longer isolated machines,  
> but capability-bearing nodes on your private network  
> that AI agents can directly operate.

---

## ⭐️ Star this repo if this resonates

We're building the runtime layer for AI to interact with the real world.
