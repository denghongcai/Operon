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
private network.

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
It runs on top of connectivity you already trust.

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
| capabilities: fs | exec | service                          |
| controls:     policy | graph                              |
+-----------------------------------------------------------+
                              |
         secure access over existing reachability
                              |
+-----------------------------------------------------------+
|                 Existing private network                  |
| Cloudflare Mesh | Tailscale | WireGuard | LAN | Kubernetes|
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
- A clear boundary between private networking and capability authorization.

Cloudflare Mesh or Tailscale can decide whether one device can reach another
device. Operon decides whether that device can read a directory, run a command, use
a secret, forward a local service, or inspect an execution trace.

---

## Quickstart

Install the latest release binary. Linux and macOS release archives use
`.tar.gz`; Windows release archives use `.zip`. Live mount support is available
through Linux FUSE, macOS FUSE-T, and Windows WinFsp; macOS and Windows hosts
must have the corresponding platform runtime installed before `operon mount`
can start a filesystem session.

The prebuilt Linux archives are glibc-based and currently target glibc 2.31 or
newer, such as Ubuntu 20.04+.

```bash
VERSION="${OPERON_VERSION:-$(curl -fsSL https://api.github.com/repos/denghongcai/Operon/releases/latest | sed -n 's/.*"tag_name": "\(v[^"]*\)".*/\1/p')}"
test -n "$VERSION" || { echo "failed to resolve latest Operon release" >&2; exit 1; }
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) ARCH=linux-x86_64 ;;
  Linux-aarch64|Linux-arm64) ARCH=linux-arm64 ;;
  Linux-armv7l|Linux-armv7*) ARCH=linux-armv7 ;;
  Darwin-x86_64) ARCH=macos-x86_64 ;;
  Darwin-arm64) ARCH=macos-aarch64 ;;
  *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

curl -fL "https://github.com/denghongcai/Operon/releases/download/${VERSION}/operon-${VERSION}-${ARCH}.tar.gz" -o /tmp/operon.tar.gz
tar -xzf /tmp/operon.tar.gz -C /tmp
sudo install "/tmp/operon-${VERSION}-${ARCH}/operon" /usr/local/bin/operon
sudo install "/tmp/operon-${VERSION}-${ARCH}/operond" /usr/local/bin/operond
```

On Windows PowerShell:

```powershell
$Version = if ($env:OPERON_VERSION) { $env:OPERON_VERSION } else { (Invoke-RestMethod https://api.github.com/repos/denghongcai/Operon/releases/latest).tag_name }
$Arch = "windows-x86_64"
$Archive = "$env:TEMP\operon.zip"
$InstallRoot = "$env:LOCALAPPDATA\Operon"
Invoke-WebRequest "https://github.com/denghongcai/Operon/releases/download/$Version/operon-$Version-$Arch.zip" -OutFile $Archive
Expand-Archive -Path $Archive -DestinationPath $env:TEMP -Force
New-Item -ItemType Directory -Force -Path $InstallRoot | Out-Null
Copy-Item "$env:TEMP\operon-$Version-$Arch\operon.exe" $InstallRoot -Force
Copy-Item "$env:TEMP\operon-$Version-$Arch\operond.exe" $InstallRoot -Force
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

Start the daemon. This command runs in the foreground:

```bash
operond start
```

For a managed local daemon on Linux or macOS, install a platform-native user
service that still runs the same foreground daemon entrypoint under systemd or
launchd supervision:

```bash
operond service install --config "$HOME/.operon/config.yaml"
operond service start
operond service status
```

`operond service stop` and `operond service uninstall` control the same
supervisor entry. On Windows, `operond service install --config <path>`
registers a real Windows Service entrypoint and may require an elevated shell
depending on local service-control policy.

In another terminal, verify the local node:

```bash
operon node ping local
operon capability list local
```

`operond` and `operon` read `$HOME/.operon/config.yaml` by default. Put daemon
endpoints on an existing private network such as Cloudflare Mesh, Tailscale,
WireGuard, LAN, or Kubernetes networking before exposing them to other machines.

Optional shell completions:

```bash
mkdir -p ~/.local/share/bash-completion/completions
operon completion bash > ~/.local/share/bash-completion/completions/operon

mkdir -p ~/.zfunc
operon completion zsh > ~/.zfunc/_operon
```

Optional agent skills require Node.js 18 or newer, npm/npx, and git:

```bash
npx -y skills add https://github.com/denghongcai/Operon --list
npx -y skills add https://github.com/denghongcai/Operon --skill '*' --agent codex --yes
```

This uses the [Vercel Skills CLI](https://github.com/vercel-labs/skills) to
install Operon's repo-local skills for agents such as Codex, Claude Code,
Cursor, and other supported coding agents. Replace `codex` with your target
agent, or omit `--agent codex --yes` to choose interactively.

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
EXEC_ID="$(operon --json exec run local -- echo hello | sed -n 's/.*"id": "\([^"]*\)".*/\1/p' | head -n 1)"
operon exec logs local "$EXEC_ID"
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
for example `operon --json exec list local`.

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
- `policy`: allowed filesystem mounts, exec roots, services, and secrets.
- `secrets`: file-backed secret references for exec injection.

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
network overlays, assign mesh IPs, perform NAT traversal, or grant capability
access through discovery.

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

Current capability areas:

- Filesystem read, write, list, copy, mutation, and live mount access through
  Linux FUSE, macOS FUSE-T, or Windows WinFsp.
- Exec execution with logs, stdin, cancellation, timeouts, scoped secrets, and
  Unix-like PTY-backed interactive sessions when policy enables `exec.session`.
  Windows interactive exec sessions are explicitly unsupported in this release
  line; use non-interactive `exec run` on Windows.
- Service metadata, TCP health checks, TCP forwarding, and UDP/datagram
  forwarding over existing Operon node connections.
- Audit, trace, and graph inspection.

`operon capability list <node>` is policy-derived: filesystem capabilities come
from configured mounts, exec capability appears only when policy allows at least
one working directory, and service capabilities come from configured services
and their permissions.

Use `operon capability explain <node> <capability_id> <action> <resource>` to
ask a daemon why one action is allowed or denied.

For first-pass troubleshooting, run `operon doctor` or
`operon --json doctor`. Doctor reports config warnings, endpoint/auth health,
runtime protocol version mismatches, capability diagnostics, and service health
checks from one command. Doctor also reports platform caveats: mount adapter
runtime requirements, private token/config file protection, exec cancellation
guarantees, PTY session support, and service forwarding firewall sensitivity.
Windows
token and config private-file handling uses ACL-aware validation for files
generated by Operon, allowing the current user, Administrators, and SYSTEM
while rejecting broadly accessible existing private files. Windows PTY sessions
report `windows-exec-session-unsupported`.

For daemon process management, `operond service status` delegates to the
platform supervisor: user-level systemd on Linux, launchd user agents on macOS,
and the Windows Service Control Manager on Windows. `operond start` remains the
foreground runtime command and does not have a `--background` mode.

---

## Secure By Design

Operon enforces capability boundaries:

- Nodes expose only explicit mounts and configured services.
- Secrets are injected only into allowed execs that request them.
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

Operon is usable today as a pre-1.0 runtime. The current release includes:

- Rust daemon and CLI.
- gRPC runtime protocol.
- TypeScript SDK.
- Unified config and guided onboarding.
- Policy-derived capabilities.
- Filesystem, exec, service, audit, trace, and graph flows.
- Live mount support through Linux FUSE, macOS FUSE-T, and Windows WinFsp.
- TCP and UDP service forwarding over existing node connections.
- mDNS endpoint discovery for local networks.

Current public release artifacts cover Linux `x86_64`, Linux `arm64`, Linux
`armv7`, macOS `x86_64`, macOS `aarch64`, and Windows `x86_64`. macOS and
Windows prebuilt archives include daemon/CLI, gRPC, config, filesystem RPC,
exec, service, audit, trace, graph, SDK protocol flows, and platform live mount
adapters. Windows non-interactive exec cancellation uses Job Object process-tree
termination. macOS live mounts require FUSE-T on the host. Windows live mounts
require WinFsp on the host.

For contributor setup, validation commands, release automation, detailed config
reference, and current phase tracking, see:

- [DEVELOPMENT.md](DEVELOPMENT.md)
- [PROTOCOL.md](PROTOCOL.md)
- [Runtime API Architecture](docs/architecture/runtime-api.md)
- [Development Phases](docs/plan/development-phases.md)

---

## Vision

Operon is not a remote control tool or network overlay.

It is a new execution model where:

> Computers are no longer isolated machines,  
> but capability-bearing nodes on your private network  
> that AI agents can directly operate.

---

## Star This Repo If This Resonates

We're building the runtime layer for AI to interact with the real world.
