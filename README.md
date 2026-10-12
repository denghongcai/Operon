<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/operon-full-logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="assets/operon-full-logo-light.svg">
    <img alt="Operon" src="assets/operon-full-logo-light.svg" width="420">
  </picture>
</h1>

[![Latest Release](https://img.shields.io/github/v/release/denghongcai/Operon?sort=semver&display_name=tag)](https://github.com/denghongcai/Operon/releases/latest)

> The missing execution model for AI agents.

Operon is an AI-native capability runtime for your machines. It uses the
network you already have (Tailscale, WireGuard, Cloudflare Mesh, LAN,
Kubernetes…), or connects nodes itself with built-in peer-to-peer
connectivity, backed by [iroh](https://iroh.computer).
Either way, reachability never implies permission: policy decides what each
node can do, and every action is traced and audited.

---

## What Is Operon?

Inspired by operons in biology, an **Operon** is a unit of coordinated
execution:

- A **node** is any reachable machine: local, cloud, container, or server.
- A **capability** is something a node can do: filesystem access, command
  execution, service access.
- An **operon** is a composition of capabilities executed across nodes.

```text
Operon = Capability + Context + Policy + Execution
```

For example, an agent can write files into a remote workspace, run validation
commands on the machine that owns it, and inspect the resulting trace. Or it can
start a temporary service with `exec.run`, verify it through a policy-controlled
health check or local forward, and keep that path auditable.

Operon is not a VPN, remote desktop, file sync tool, or SSH wrapper.
Connectivity is a means; Operon is about what connected machines are allowed to
do, how execution is composed, and how results are traced.

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
| discover -> authorize -> execute -> trace + audit         |
|                                                           |
| capabilities: fs | exec | service                         |
| controls:     policy | graph                              |
+-----------------------------------------------------------+
                              |
             authenticated gRPC between nodes
                              |
+-----------------------------------------------------------+
| existing network (LAN, VPN, K8s) or built-in connectivity |
+-----------------------------------------------------------+
      |                       |                       |
   node-a                  node-b                  node-c
 fs + exec                 service                fs + exec
```

---

## Quickstart

Install `operon` (CLI) and `operond` (daemon) from the
[latest release](https://github.com/denghongcai/Operon/releases/latest).
[docs/install.md](docs/install.md) has copy-paste install scripts for Linux,
macOS, and Windows, plus libc selection, live mount prerequisites, shell
completions, and agent skills.

Create a local workspace and config:

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

Start the daemon in the foreground (or install it as a
[managed service](docs/install.md#managed-daemon-service)):

```bash
operond start
```

In another terminal, verify the local node:

```bash
operon node ping local
operon capability list local
```

To give your coding agent Operon skills:

```bash
npx -y skills add https://github.com/denghongcai/Operon --skill '*' --agent codex --yes
```

---

## Basic Usage

Inspect nodes and capabilities:

```bash
operon node list
operon capability list local
operon capability explain local fs:workspace read /
```

Read and write files inside configured workspace mounts:

```bash
operon fs list local:/
operon fs write local:/notes.txt --content "hello from Operon"
operon fs read local:/notes.txt
```

Run commands with policy-controlled working directories, timeouts, and secrets:

```bash
operon exec run local -- echo hello
operon exec run local --argv -- printf "hello world"
operon exec list local
operon exec logs local <exec-id>
```

Check services and open explicit local forwards (runs in the foreground):

```bash
operon service list local
operon service check local local-daemon
operon service forward local local-daemon --listen 127.0.0.1:17789
```

Review audit output:

```bash
operon audit show local --limit 20
```

Add global `--json` before the subcommand for structured output, for example
`operon --json exec list local`. When something looks wrong, run
`operon doctor`.

### Workflows

A workflow composes node capabilities into a traceable execution graph:

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

```bash
operon run --trace-output ./trace.json ./workflow.yaml
operon trace show ./trace.json
```

Every run records inputs, outputs, logs, duration, status, and policy decisions.

---

## Capabilities

- **Filesystem:** read, write, list, copy, mutation, and live mount through
  Linux FUSE, macOS FUSE-T, and Windows WinFsp.
- **Exec:** logs, stdin, cancellation, timeouts, scoped secrets, and
  PTY-backed interactive sessions.
- **Service:** metadata, TCP health checks, TCP and UDP forwarding over
  existing node connections.
- **Audit, trace, and graph** inspection.

`operon capability list <node>` is policy-derived: filesystem capabilities come
from configured mounts, exec appears only when policy allows at least one
working directory, and service capabilities come from configured services and
their permissions. `operon capability explain` tells you why one action is
allowed or denied.

---

## Connecting Nodes

`operon onboard` writes a unified `$HOME/.operon/config.yaml` with `daemon`,
`client`, `policy`, and `secrets` sections; `operon config explain` describes
it. Nodes are addressed in one of two ways.

**Over an existing network** (default). Point clients at daemon endpoints on
your LAN, VPN, or Kubernetes network:

```yaml
client:
  nodes:
    gpu-node:
      endpoint: grpc://100.96.18.20:7789
```

**With built-in connectivity** (optional, backed by iroh). No VPN or separate
connectivity client is needed. The daemon adds an iroh listener and prints its
public endpoint ID at startup:

```yaml
daemon:
  iroh:
    identity_file: iroh-server.key
    bind_addrs: ["0.0.0.0:7788"]
```

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

Cross-network access uses explicitly configured self-hosted relays; see the
[built-in connectivity guide](docs/networking/embedded-iroh.md).

Filesystem, exec, and mount commands are the same either way. Before binding a
daemon beyond loopback, configure bearer-token auth with
`daemon.auth.token_file` or `daemon.auth.token_env`. LAN mDNS discovery can find
local daemons, but discovery never grants capability access. Timeouts for slow
links are configurable per node; see
[transport liveness configuration](PROTOCOL.md#transport-liveness-configuration).

---

## Security Model

- Network reachability, discovery, and peer authentication never imply
  capability permission.
- Nodes expose only explicit filesystem mounts, exec roots, and services.
  Service permissions are default-deny when omitted; set `permissions.check`
  and/or `permissions.forward` explicitly.
- Secrets are injected only into allowed execs that request them.
- Every action is auditable.

See [security hardening](docs/quality/security-hardening.md) for hardening and
upgrade notes.

---

## Status and Docs

Operon is a usable pre-1.0 runtime: Rust daemon and CLI, gRPC protocol,
TypeScript SDK, and prebuilt releases for Linux, macOS, and Windows.

- [Installing Operon](docs/install.md)
- [Platform support, diagnostics, and live mount tuning](docs/platforms.md)
- [Built-in connectivity (iroh)](docs/networking/embedded-iroh.md)
- [DEVELOPMENT.md](DEVELOPMENT.md): config reference, contributing, validation, releases
- [PROTOCOL.md](PROTOCOL.md): direct gRPC integration
- [Runtime API architecture](docs/architecture/runtime-api.md)
- [Development phases](docs/plan/development-phases.md)

---

## Vision

> Computers are no longer isolated machines,  
> but capability-bearing nodes  
> that AI agents can directly operate.

If this resonates, star the repo.
