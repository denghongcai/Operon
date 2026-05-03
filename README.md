# Operon

> The missing execution model for AI agents.

Operon is an AI-native capability runtime for machines already connected by
Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, Kubernetes networking, or any
other private network.

Instead of building another VPN or mesh network, Operon focuses on what happens
after machines can reach each other: capability discovery, policy, execution
graphs, audit, and agent-friendly tooling.

---

## What Is Operon?

Inspired by the concept of operons in biology, an **Operon** is a unit of
coordinated execution.

In Operon:

- A **node** is any reachable machine: local, cloud, container, or server.
- A **capability** is something a node can do: filesystem access, job
  execution, service access, and related runtime actions.
- An **operon** is a composition of capabilities executed across nodes.

```text
Operon = Capability + Context + Policy + Execution
```

Operon is not a VPN, mesh network, remote desktop, file sync tool, or SSH
wrapper. It runs on top of connectivity you already trust.

---

## Why Operon?

AI agents often interact with real machines through fragmented tools:

- SSH
- APIs
- File uploads
- Remote desktops
- VPN-connected machines

That usually means poor composability, weak execution traces, unclear security
boundaries, and network access being mistaken for capability access.

Operon adds:

- A unified capability model.
- Structured execution across reachable nodes.
- Built-in trace and audit output.
- Policy-controlled filesystem, job, service, and secret use.
- A clear boundary between private networking and capability authorization.

Cloudflare Mesh or Tailscale can decide whether one device can reach another
device. Operon decides whether that device can read a directory, run a job, use
a secret, forward a local service, or inspect an execution trace.

---

## Quickstart

Install the latest Linux release binary:

```bash
VERSION="${OPERON_VERSION:-$(curl -fsSL https://api.github.com/repos/denghongcai/Operon/releases/latest | sed -n 's/.*"tag_name": "\(v[^"]*\)".*/\1/p')}"
test -n "$VERSION" || { echo "failed to resolve latest Operon release" >&2; exit 1; }
case "$(uname -m)" in
  x86_64) ARCH=linux-x86_64 ;;
  aarch64|arm64) ARCH=linux-arm64 ;;
  armv7l|armv7*) ARCH=linux-armv7 ;;
  *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

curl -fL "https://github.com/denghongcai/Operon/releases/download/${VERSION}/operon-${VERSION}-${ARCH}.tar.gz" -o /tmp/operon.tar.gz
tar -xzf /tmp/operon.tar.gz -C /tmp
sudo install "/tmp/operon-${VERSION}-${ARCH}/operon" /usr/local/bin/operon
sudo install "/tmp/operon-${VERSION}-${ARCH}/operond" /usr/local/bin/operond
```

Create a local workspace and guided config:

```bash
mkdir -p "$HOME/operon-workspace" "$HOME/.operon"
operon onboard \
  --yes \
  --role both \
  --output-dir "$HOME/.operon" \
  --node-id local \
  --workspace "$HOME/operon-workspace" \
  --listen 127.0.0.1:7789
```

Start the daemon and verify the local node:

```bash
operond start
operon node ping local
operon capability list local
```

`operond` and `operon` read `$HOME/.operon/config.yaml` by default. Put daemon
endpoints on an existing private network such as Cloudflare Mesh, Tailscale,
WireGuard, SSH, LAN, or Kubernetes networking before exposing them to other
machines.

Optional shell completions:

```bash
mkdir -p ~/.local/share/bash-completion/completions
operon completion bash > ~/.local/share/bash-completion/completions/operon

mkdir -p ~/.zfunc
operon completion zsh > ~/.zfunc/_operon
```

---

## Basic Usage

Inspect nodes and capabilities:

```bash
operon node list
operon node ping local
operon capability list local
operon capability explain local fs:workspace read /
```

Read and write files inside configured workspace mounts:

```bash
operon fs list local:/
operon fs read local:/README.md
operon fs write local:/notes.txt --content "hello from Operon"
```

Run jobs with policy-controlled working directories, timeouts, and secret use:

```bash
operon job run local -- echo hello
operon job run local --argv -- printf "hello world"
operon job list local
operon job logs local job-1
```

Inspect configured services and open explicit local forwards:

```bash
operon service list local
operon service check local daemon
operon service forward local daemon --listen 127.0.0.1:17789
```

Review audit and trace output:

```bash
operon audit show local --limit 20
operon run --trace-output ./trace.json ./workflow.yaml
operon trace show ./trace.json
```

Add `--json` for structured output when scripting.

---

## Configuration

`operon onboard` writes a unified `config.yaml` plus referenced secret files
where needed:

```bash
operon onboard
operon config explain
```

The config contains:

- `daemon`: local node id, listen address, workspace, auth, and store settings.
- `client`: known node endpoints such as `grpc://100.96.12.34:7789`.
- `policy`: allowed filesystem mounts, job roots, services, and secrets.
- `secrets`: file-backed secret references for job injection.

External control planes can generate the same endpoint-only `client.nodes`
shape from Cloudflare, Tailscale, Kubernetes, inventory databases, or DNS.
Discovery and generated endpoints do not grant capability access; policy still
controls what each node can do.

For full config, policy, command, validation, release, and protocol reference,
see [DEVELOPMENT.md](DEVELOPMENT.md).

---

## Network Boundary

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
version: 1

client:
  nodes:
    cloud-a:
      endpoint: grpc://100.96.12.34:7789
    gpu-node:
      endpoint: grpc://100.96.18.20:7789
```

In production-style deployments, run daemon endpoints only on an existing
encrypted private network or behind a trusted local tunnel.

LAN mDNS discovery can find local Operon daemons, but Operon does not create
VPNs, assign mesh IPs, perform NAT traversal, or grant capability access through
discovery.

---

## Example Workflow

An Operon workflow composes node capabilities into a traceable execution graph:

```yaml
name: copy-and-run

steps:
  - id: write-input
    node: cloud-a
    action: fs.write
    path: /graph-input.txt
    content: hello from graph

  - id: run-command
    node: gpu-node
    action: job.run
    cwd: /
    timeout_secs: 30
    command: cat graph-input.txt > graph-output.txt

  - id: read-output
    node: cloud-a
    action: fs.read
    path: /graph-output.txt
```

Run it against already reachable daemon endpoints:

```bash
operon --config ./operon.config.yaml run --trace-output ./trace.json ./workflow.yaml
operon trace show ./trace.json
```

Every run records inputs, outputs, logs, duration, status, and policy decisions
that can be inspected by humans, scripts, or agents.

---

## Capabilities

Operon exposes machines through explicit capabilities:

```text
mesh://cloud-a/fs/workspace
mesh://gpu-node/job/run
mesh://cloud-a/service/web
```

Current capability areas:

- Filesystem read, write, list, copy, mutation, and Linux FUSE mount access.
- Job execution with logs, stdin, cancellation, timeouts, and scoped secrets.
- Service metadata, TCP health checks, TCP forwarding, and UDP/datagram
  forwarding over existing Operon node connections.
- Audit, trace, and graph inspection.

`operon capability list <node>` is policy-derived: filesystem capabilities come
from configured mounts, job capability appears only when policy allows at least
one working directory, and service capabilities come from configured services
and their permissions.

Use `operon capability explain <node> <capability_id> <action> <resource>` to
ask a daemon why one action is allowed or denied.

---

## Secure By Design

Operon enforces capability boundaries:

- Nodes expose only explicit mounts and configured services.
- Secrets are injected only into allowed jobs that request them.
- Execution is policy-controlled.
- Every action is auditable.
- Network reachability never implies capability permission.

---

## AI-Native

Operon is designed for agents, not just humans.

Agents can:

- discover available capabilities
- compose workflows dynamically
- execute across multiple reachable nodes
- inspect audit and trace output
- ask why a capability action is allowed or denied before attempting it

For SDK and direct protocol integration details, see
[DEVELOPMENT.md](DEVELOPMENT.md) and [PROTOCOL.md](PROTOCOL.md).

---

## Project Status

Operon is in early development. The current runtime includes:

- Rust daemon and CLI.
- gRPC runtime protocol.
- TypeScript SDK.
- Unified config and guided onboarding.
- Policy-derived capabilities.
- Filesystem, job, service, audit, trace, and graph flows.
- Linux FUSE mount support.
- TCP and UDP service forwarding over existing node connections.
- mDNS endpoint discovery for local networks.

For contributor setup, validation commands, release automation, detailed config
reference, and current phase tracking, see:

- [DEVELOPMENT.md](DEVELOPMENT.md)
- [PROTOCOL.md](PROTOCOL.md)
- [Runtime API Architecture](docs/architecture/runtime-api.md)
- [Development Phases](docs/plan/development-phases.md)

---

## Vision

Operon is not a VPN or remote control tool.

It is a new execution model where:

> Computers are no longer isolated machines,  
> but capability-bearing nodes on your private network  
> that AI agents can directly operate.

---

## Star This Repo If This Resonates

We're building the runtime layer for AI to interact with the real world.
