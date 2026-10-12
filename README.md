<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/operon-full-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="assets/operon-full-logo-light.svg">
    <img alt="Operon" src="assets/operon-full-logo-light.svg" width="420">
  </picture>
</h1>

[![Latest Release](https://img.shields.io/github/v/release/denghongcai/Operon?sort=semver&display_name=tag)](https://github.com/denghongcai/Operon/releases/latest)

> The missing execution model for AI agents.

Operon is an AI-native capability runtime for machines already connected by
Cloudflare Mesh, Tailscale, WireGuard, LAN, Kubernetes networking, or any other
private network — or connected by Operon itself, through built-in peer-to-peer
connectivity backed by [iroh](https://iroh.computer).

Operon focuses on what happens after machines can reach each other: capability
discovery, policy, execution graphs, audit, and agent-friendly tooling.

---

## What Is Operon?

Inspired by the concept of operons in biology, an **Operon** is a unit of
coordinated execution.

In Operon:

- A **node** is any reachable machine: local, cloud, container, or server.
- A **capability** is something a node can do: filesystem access, command
  execution, service access, and related runtime actions.
- An **operon** is a composition of capabilities executed across nodes.

```text
Operon = Capability + Context + Policy + Execution
```

For example:

- **Agent workspace handoff:** an agent can write files into a remote workspace,
  run validation commands on the machine that owns that workspace, and inspect
  the resulting trace and audit events.
- **Remote service verification:** an agent can start a temporary service on a
  remote machine with `exec.run`, verify it with a policy-controlled service
  health check or explicit local forward, and keep that operational path
  auditable.

Operon is not a network overlay, remote desktop, file sync tool, or SSH wrapper.
It turns reachable machines into nodes with explicit, auditable capabilities.

---

## Why Operon?

AI agents often interact with real machines through fragmented tools:

- SSH
- APIs
- File uploads
- Remote desktops
- Privately reachable machines

That usually means poor composability, weak execution traces, unclear security
boundaries, and network access being mistaken for capability access.

```text
+---------------------------+   +---------------------------+
| AI agents                 |   | apps and tools            |
| use CLI + skills          |   | use SDK                   |
+-------------+-------------+   +-------------+-------------+
              |                               |
              +---------------+---------------+
                              |
                              v
+-----------------------------------------------------------+
| Operon capability runtime                                 |
|                                                           |
| discover -> authorize -> execute                          |
|                 |                                         |
|                 v                                         |
| trace + audit                                             |
|                                                           |
| capabilities: fs | exec | service                         |
| controls:     policy | graph                              |
+-----------------------------------------------------------+
                              |
              secure access over any reachability
                              |
+-----------------------------------------------------------+
|  existing private network      |  built-in connectivity   |
|  Mesh | Tailscale | WireGuard  |  peer-to-peer, by iroh   |
|  LAN | Kubernetes              |                          |
+-----------------------------------------------------------+
      |                       |                       |
   node-a                  node-b                  node-c
 fs + execs                service                fs + execs
```

Operon adds:

- A unified capability model.
- Structured execution across reachable nodes.
- Built-in trace and audit output.
- Policy-controlled filesystem, exec, service, and secret use.
- A clear boundary between connectivity and capability authorization.

Cloudflare Mesh, Tailscale, or Operon's built-in connectivity can decide whether
one device can reach another device. Operon's policy decides whether that device
can read a directory, run a command, use a secret, forward a local service, or
inspect an execution trace.

---

## Quickstart

Install `operon` (CLI) and `operond` (daemon) on Linux or macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/denghongcai/Operon/main/scripts/install.sh | bash
```

On Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/denghongcai/Operon/main/scripts/install.ps1 | iex
```

The installers pick the right archive for your platform and libc, verify its
checksum, and accept `OPERON_VERSION` and `OPERON_INSTALL_DIR`. See
[docs/install.md](docs/install.md) for manual install, Alpine, and live mount
prerequisites.

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

Start the daemon. This command runs in the foreground (to run it as a system
service, see [managed daemon service](docs/install.md#managed-daemon-service)):

```bash
operond start
```

In another terminal, verify the local node:

```bash
operon node ping local
operon capability list local
```

Give your coding agent Operon skills (Node.js 18+; replace `codex` with your
target agent):

```bash
npx -y skills add https://github.com/denghongcai/Operon --skill '*' --agent codex --yes
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
operon fs write local:/notes.txt --content "hello from Operon"
operon fs read local:/notes.txt
```

Run commands with policy-controlled working directories, timeouts, and secret use:

```bash
operon exec run local -- echo hello
operon exec run local --argv -- printf "hello world"
operon exec list local
operon exec logs local <exec-id>
```

Inspect configured services and open explicit local forwards. The forward
command runs in the foreground; stop it with Ctrl-C when finished:

```bash
operon service list local
operon service check local local-daemon
operon service forward local local-daemon --listen 127.0.0.1:17789
```

Review audit output:

```bash
operon audit show local --limit 20
```

Add global `--json` before the subcommand for structured output when scripting,
for example `operon --json exec list local`. When something looks wrong, start
with `operon doctor`.

---

## Example Workflow

An Operon workflow composes node capabilities into a traceable execution graph.
This example runs against the `local` node created in the Quickstart:

```yaml
name: local-copy-and-run

steps:
  - id: write-input
    node: local
    action: fs.write
    path: /graph-input.txt
    content: hello from graph

  - id: run-command
    node: local
    action: exec.run
    cwd: /
    timeout_secs: 30
    command: cat graph-input.txt > graph-output.txt

  - id: read-output
    node: local
    action: fs.read
    path: /graph-output.txt
```

Save it as `workflow.yaml`, then run it:

```bash
operon run --trace-output ./trace.json ./workflow.yaml
operon trace show ./trace.json
```

Every run records inputs, outputs, logs, duration, status, and policy decisions
that can be inspected by humans, scripts, or agents.

---

## Capabilities

Operon exposes machines through explicit capabilities:

```text
mesh://cloud-a/fs/workspace
mesh://gpu-node/exec/run
mesh://cloud-a/service/web
```

- **Filesystem:** read, write, list, copy, mutation, and live mount through
  Linux FUSE, macOS FUSE-T, and Windows WinFsp.
- **Exec:** logs, stdin, cancellation, timeouts, scoped secrets, and
  PTY-backed interactive sessions.
- **Service:** metadata, TCP health checks, and TCP and UDP forwarding over
  existing node connections.
- **Audit, trace, and graph** inspection.

`operon capability list <node>` is policy-derived: filesystem capabilities come
from configured mounts, exec capability appears only when policy allows at least
one working directory, and service capabilities come from configured services
and their permissions. Use `operon capability explain` to ask a daemon why one
action is allowed or denied.

Platform notes, diagnostics, and live mount tuning are in
[docs/platforms.md](docs/platforms.md).

---

## Connecting Nodes

`operon onboard` writes a unified `config.yaml` with `daemon`, `client`,
`policy`, and `secrets` sections; `operon config explain` describes it. Nodes
are addressed in one of two ways.

**Over your existing network.** Point clients at daemon endpoints on Cloudflare
Mesh, Tailscale, WireGuard, LAN, or Kubernetes networking:

```yaml
client:
  nodes:
    cloud-a:
      endpoint: grpc://100.96.12.34:7789
    gpu-node:
      endpoint: grpc://100.96.18.20:7789
```

**Through built-in connectivity.** No VPN or separate connectivity client is
needed. The daemon adds a listener and prints its public endpoint ID at startup:

```yaml
daemon:
  iroh:
    identity_file: iroh-server.key
    bind_addrs: ["0.0.0.0:7788"]
```

Clients address the node by that ID:

```yaml
client:
  nodes:
    remote:
      endpoint: iroh://SERVER_PUBLIC_ENDPOINT_ID
      auth:
        token_file: remote.token
      transport:
        iroh:
          identity_file: iroh-client.key
          peer_addrs: ["192.168.1.20:7788"]
```

Filesystem, exec, and mount commands are the same either way. For access across
networks, configure self-hosted relays; see the
[built-in connectivity guide](docs/networking/embedded-iroh.md).

Before binding a daemon beyond loopback, configure bearer-token auth with
`daemon.auth.token_file` or `daemon.auth.token_env`. Timeouts for slow or
international links are configurable per node; see
[transport liveness configuration](PROTOCOL.md#transport-liveness-configuration).
For the full config reference, see [DEVELOPMENT.md](DEVELOPMENT.md).

---

## Secure By Design

Operon enforces capability boundaries:

- Nodes expose only explicit mounts and configured services.
- Service permissions are default-deny when omitted.
- Secrets are injected only into allowed execs that request them.
- Execution is policy-controlled.
- Every action is auditable.
- Reachability, discovery, and peer authentication never imply capability
  permission.

Hardening and upgrade notes live in
[docs/quality/security-hardening.md](docs/quality/security-hardening.md).

---

## AI-Native

Operon is designed for agents, not just humans.

Agents can:

- discover available capabilities
- compose workflows dynamically
- execute across multiple reachable nodes
- inspect audit and trace output
- ask why a capability action is allowed or denied before attempting it

For SDK and direct protocol integration, see [DEVELOPMENT.md](DEVELOPMENT.md)
and [PROTOCOL.md](PROTOCOL.md).

---

## Project Status

Operon is usable today as a pre-1.0 runtime: Rust daemon and CLI, gRPC runtime
protocol, TypeScript SDK, and prebuilt releases for Linux, macOS, and Windows.

- [Installing Operon](docs/install.md)
- [Platform support, diagnostics, and live mount tuning](docs/platforms.md)
- [Built-in connectivity](docs/networking/embedded-iroh.md)
- [DEVELOPMENT.md](DEVELOPMENT.md): config reference, contributing, validation, releases
- [PROTOCOL.md](PROTOCOL.md): direct gRPC integration
- [Runtime API architecture](docs/architecture/runtime-api.md)
- [Development phases](docs/plan/development-phases.md)

---

## Vision

Operon is not a remote control tool or network overlay.

It is a new execution model where:

> Computers are no longer isolated machines,  
> but capability-bearing nodes on your network  
> that AI agents can directly operate.

---

## Star This Repo If This Resonates

We're building the runtime layer for AI to interact with the real world.
