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

- Rust stable toolchain, 1.85 or newer
- Node.js and pnpm
- Docker with Docker Compose
- `/dev/fuse` and `fusermount3` for Linux mount validation

Run the full v0.6 validation:

```bash
pnpm install --frozen-lockfile
cargo fmt --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --locked -- -D warnings
pnpm typecheck
pnpm test
scripts/verify-v0.5-docker.sh
scripts/verify-v0.6-linux-mount.sh
scripts/verify-v0.6.1-linux-write-mount.sh
scripts/verify-v0.6.2-cli-fs-cleanup.sh
scripts/verify-v0.6.3-fs-copy.sh
```

The Docker validation starts two reachable `operond` nodes, exercises capabilities through the CLI, checks auth, policy, audit filters, store queries, secret use, service health checks, streaming fs, job stdin/log streams, LAN mDNS discovery, and runs the example execution graph over gRPC endpoints. The Linux mount validation adds a real FUSE mount read check when the host has `/dev/fuse`; otherwise it reports the missing host requirement and exits cleanly.
The v0.6.1 Linux write mount validation checks create, write, read-after-write,
truncate, delete, rename, denied write/delete/rename audit, and cleanup.
The v0.6.2 CLI fs cleanup validation checks direct CLI mutation commands for
mkdir, truncate, rename, rm, denied mutations, and audit.
The v0.6.3 fs copy validation checks same-node daemon-side copy, denied copy,
and audit.

---

## CLI and Configuration

The v0.5 runtime has two binaries:

- `operond`: the daemon that runs on each reachable machine.
- `operon`: the CLI that talks to daemon endpoints.

From the repo, run them through Cargo:

```bash
cargo run -p operond -- start --grpc-listen 0.0.0.0:7789 --node-id local --workspace /workspace
cargo run -p operon-cli -- --config examples/nodes.yaml node list
```

After installing built binaries, the same commands are:

```bash
operond start --grpc-listen 0.0.0.0:7789 --node-id local --workspace /workspace
operon --config examples/nodes.yaml node list
```

### Node Config

The CLI reads node endpoints from a YAML file. In v0.5, the default path is:

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
    endpoint: grpc://127.0.0.1:7789
    token: local-dev-token
  cloud-a:
    endpoint: grpc://100.96.12.34:7789
    provider: tailscale
  gpu-node:
    endpoint: grpc://100.96.18.20:7789
    provider: cloudflare-mesh
```

`endpoint` may be `grpc://` or `grpcs://`. The CLI uses gRPC for runtime operations. Use `operon --json` for scripts, and use `PROTOCOL.md` if you need to integrate without an SDK. `provider` is optional and defaults to `manual`. `token` is optional and is sent as a bearer token when the target daemon is started with `--auth-token` or `--auth-token-file`.

Provider support remains endpoint-oriented. LAN mDNS discovery can find local Operon daemons, but Operon still does not create VPNs, assign mesh IPs, or grant capability access through discovery.

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
  --grpc-listen 0.0.0.0:7789 \
  --node-id cloud-a \
  --workspace /home/ubuntu/workspace \
  --policy ./operon.policy.yaml \
  --auth-token-file ./operon.token \
  --store ./operon-store.jsonl \
  --secrets ./operon.secrets.yaml \
  --advertise-lan
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
  allowed_secrets:
    - GITHUB_TOKEN

service:
  services:
    - id: daemon
      name: daemon
      host: 127.0.0.1
      port: 7789
      protocol: tcp
      description: Operon gRPC daemon listener
```

Policy paths are virtual paths inside the daemon workspace. If the daemon starts with `--workspace /home/ubuntu/workspace`, policy path `/` means that workspace root, not the host root.

Secret file shape:

```yaml
GITHUB_TOKEN: ghp_example
```

Secrets are only injected into jobs that request them and are allowed by policy. The daemon does not expose a secret read API; audit output records secret names, not values.

### Common Commands

```bash
operon --config ./operon.nodes.yaml node list
operon --config ./operon.nodes.yaml node resolve cloud-a
operon node discover --provider lan --timeout-secs 3
operon --config ./operon.nodes.yaml node ping cloud-a
operon provider list
operon --config ./operon.nodes.yaml capability list cloud-a
operon --config ./operon.nodes.yaml service list cloud-a
operon --config ./operon.nodes.yaml service check cloud-a daemon

operon init config ./operon.nodes.yaml
operon init policy ./operon.policy.yaml

operon --config ./operon.nodes.yaml fs stat cloud-a:/README.md
operon --config ./operon.nodes.yaml fs list cloud-a:/
operon --config ./operon.nodes.yaml fs read cloud-a:/input.txt
operon --config ./operon.nodes.yaml fs read cloud-a:/large.bin --output ./large.bin
operon --config ./operon.nodes.yaml fs write cloud-a:/input.txt --content "hello"
operon --config ./operon.nodes.yaml fs write cloud-a:/large.bin --file ./large.bin
operon --config ./operon.nodes.yaml fs mkdir cloud-a:/work
operon --config ./operon.nodes.yaml fs truncate cloud-a:/work/file.txt --size 0
operon --config ./operon.nodes.yaml fs rename cloud-a:/work/file.txt cloud-a:/work/renamed.txt
operon --config ./operon.nodes.yaml fs copy cloud-a:/work/renamed.txt cloud-a:/work/copied.txt
operon --config ./operon.nodes.yaml fs rm cloud-a:/work/renamed.txt

operon --config ./operon.nodes.yaml job run cloud-a -- echo hello
operon --config ./operon.nodes.yaml job run cloud-a --secret GITHUB_TOKEN -- test x'$GITHUB_TOKEN' = xexpected
operon --config ./operon.nodes.yaml job run cloud-a --detach -- sleep 10
operon --config ./operon.nodes.yaml job status cloud-a job-1
operon --config ./operon.nodes.yaml job list cloud-a
operon --config ./operon.nodes.yaml job logs cloud-a job-1
operon --config ./operon.nodes.yaml job logs cloud-a job-1 --follow
operon --config ./operon.nodes.yaml job logs cloud-a job-1 --stream
operon --config ./operon.nodes.yaml job stdin cloud-a job-1 --content "input"
operon --config ./operon.nodes.yaml job stdin cloud-a job-1 --close
operon --config ./operon.nodes.yaml job cancel cloud-a job-1

operon --config ./operon.nodes.yaml audit list cloud-a
operon --config ./operon.nodes.yaml audit show cloud-a --limit 20
operon --config ./operon.nodes.yaml audit show cloud-a --capability service:daemon --action check --allowed true --resource daemon --limit 5
operon --config ./operon.nodes.yaml run --trace-output ./trace.json examples/train-model.yaml
operon trace list .
operon trace show ./trace.json
operon --json trace show ./trace.json
operon --config ./operon.nodes.yaml mount cloud-a:/ --to ./cloud-a
```

Add `--json` for structured command output or `--quiet` to suppress non-essential output.

`operon mount` is a Linux-only foreground FUSE mount. In v0.6.1 it uses
single-writer, write-through semantics: reads, writes, truncates, mkdir,
delete, and rename are sent to the remote daemon through the Core FS Protocol.
The daemon still owns workspace path containment, policy, and audit. The host
needs `/dev/fuse` and a working `fusermount3` or equivalent FUSE setup. Press
Ctrl-C in the mounting process to unmount.

The write mount does not currently provide conflict detection. Operon does not
attach file versions, etags, locks, leases, or compare-and-swap preconditions to
filesystem writes yet. If two clients write the same path concurrently, the
visible result depends on the remote filesystem and RPC arrival order. Serialize
mutating operations at the workflow, CLI, or agent layer when deterministic
ordering matters.

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
    endpoint: grpc://100.96.12.34:7789
  gpu-node:
    endpoint: grpc://100.96.18.20:7789
```

The current CLI speaks gRPC to `grpc://` daemon endpoints. There is no direct HTTP runtime API; humans and scripts should use `operon`, including `operon --json`, and programs should use SDKs or generated clients from `proto/operon/runtime.proto`. In production-style deployments, run daemon endpoints only on an existing encrypted private network or behind a trusted local tunnel.

Cloudflare Mesh or Tailscale can decide whether one device can reach another device. Operon decides whether that device can read a directory, run a job, use a secret, or inspect an execution trace.

---

## ⚡ Example

Run the local Docker v0.5 gRPC demo:

```bash
scripts/verify-v0.5-docker.sh
```

This starts two `operond` containers with gRPC listeners, validates capability discovery, token auth, fs operations, streaming file transfer, job execution, stdin/log streams, service checks, policy denial, scoped secrets, audit output, trace summaries, and runs:

```bash
operon --config examples/docker-nodes.yaml run --trace-output /tmp/operon-docker-grpc-trace.json examples/docker-copy-and-run.yaml
```

Run the Linux FUSE mount validations:

```bash
scripts/verify-v0.6-linux-mount.sh
scripts/verify-v0.6.1-linux-write-mount.sh
scripts/verify-v0.6.2-cli-fs-cleanup.sh
scripts/verify-v0.6.3-fs-copy.sh
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
- service / port metadata and TCP health checks over an existing private network

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
  { nodeId: "cloud-a", endpoint: "grpc://100.96.12.34:7789", token: "cloud-token" },
  { nodeId: "gpu-node", endpoint: "grpc://100.96.18.20:7789", token: "gpu-token" }
]);

const trace = await operon.run({
  name: "train-model",
  steps: [
    { node: "cloud-a", action: "fs.read", path: "/data" },
    { node: "gpu-node", action: "job.run", command: "train.py", secrets: ["WANDB_API_KEY"] }
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
  - gRPC
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
- [x] Token-authenticated daemon calls
- [x] Streaming-friendly fs transfer
- [x] Followed job logs
- [x] Provider endpoint resolution
- [x] JSONL audit/job store
- [x] Scoped job secrets
- [x] LAN mDNS discovery
- [x] Queryable job/audit/trace commands
- [x] Read-only mount PoC
- [x] Service / port metadata and health checks
- [x] Filtered audit and human-readable trace CLI UX
- [x] gRPC runtime protocol
- [x] Remove HTTP runtime facade
- [x] Linux read-only FUSE mount
- [x] Linux write FUSE mount
- [x] CLI fs mutation commands
- [x] Same-node fs copy
- [ ] CLI TUI console
- [ ] Agent integration
- [ ] Non-LAN provider discovery adapters

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
