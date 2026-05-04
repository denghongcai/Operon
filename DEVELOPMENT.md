# Operon Development

This document is for contributors, release maintainers, and users who need
build, validation, protocol, or detailed configuration reference. User-facing
setup and product positioning live in [README.md](README.md).

## Development Prerequisites

- Rust stable toolchain, 1.85 or newer.
- Node.js and pnpm.
- Docker with Docker Compose.
- `/dev/fuse` and `fusermount3` for Linux mount validation.

## Local Binaries

The current runtime has two binaries:

- `operond`: the daemon that runs on each reachable machine.
- `operon`: the CLI that talks to daemon endpoints.

From the repo, run them through Cargo:

```bash
cargo run -p operond -- start --config examples/config.yaml
cargo run -p operon-cli -- --config examples/config.yaml node list
cargo run -p operon-cli -- --config examples/config.yaml config explain
```

After installing built binaries, the same commands are:

```bash
operond start --config examples/config.yaml
operon --config examples/config.yaml node list
```

## Full Validation

Run the full validation:

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
scripts/verify-v0.6.4-onboard.sh
scripts/verify-v0.6.7-runtime.sh
scripts/verify-v0.6.9-cli-contract.sh
scripts/verify-v0.6.10-runtime-hardening.sh
scripts/verify-v0.6.11-governance.sh
scripts/verify-v0.6.12-runtime-boundary.sh
scripts/verify-v0.7-service-forwarding.sh
scripts/verify-v0.7.1-udp-datagram-forwarding.sh
scripts/verify-v0.8-agent-skills.sh
scripts/verify-v0.8.1-integration-coverage.sh
scripts/verify-v0.8.3-read-range-release-cleanup.sh
scripts/verify-v0.8.4-modularization.sh
scripts/verify-v0.8.5-core-domain-modules.sh
scripts/verify-v0.8.6-runtime-cli-client-modularization.sh
scripts/verify-readme-quickstart-docker.sh
scripts/verify-release-glibc-baseline.sh
scripts/verify-docs-help-skills-sync.sh
scripts/verify-v0.9-endpoint-model.sh
scripts/verify-post-v0.9-discovery-ux.sh
scripts/verify-policy-derived-capabilities.sh
scripts/verify-v0.9.3-store-backed-audit-visibility.sh
scripts/verify-v0.9.4-runtime-hardening-consolidation.sh
scripts/verify-v0.9.5-policy-language-hardening.sh
scripts/verify-v0.9.6-capability-diagnostics.sh
scripts/verify-v0.10-exec-unification.sh
scripts/verify-v0.10.1-fs-consistency-workspace-hardening.sh
scripts/verify-v0.10.2-operator-diagnostics.sh
scripts/verify-v0.11-exec-session.sh
scripts/verify-v0.10.4-maintainability-cleanup.sh
scripts/verify-v0.11.2-exec-session-hardening.sh
scripts/verify-v0.10.5-maintainability-cleanup.sh
scripts/verify-v0.11.3-platform-capability-matrix.sh
scripts/verify-v0.12-release-distribution-readiness.sh
scripts/verify-v0.12.1-platform-parity-hardening.sh
scripts/verify-v0.12.2-maintainability-cleanup.sh
scripts/verify-v0.12.3-windows-exec-process-tree-cancellation.sh
scripts/verify-v0.12.4-release-artifact-verification.sh
scripts/verify-v0.12.5-cli-grpc-maintainability-cleanup.sh
```

The README quickstart Docker validation installs the latest public release in a
fresh Ubuntu 20.04 environment, runs the user-facing Quickstart, installs the
repo-local skills through the Vercel Skills CLI with Node.js 20, and exercises
the README file, exec, service, audit, trace, and config examples.

The release glibc baseline validation keeps Linux release builds on an Ubuntu
20.04 / glibc 2.31 baseline, pins a modern `protoc` because Ubuntu 20.04's
package is too old for proto3 optional fields, and can inspect built binaries
for accidental newer GLIBC symbol requirements.

The v0.12 release distribution readiness validation checks that draft releases
build Linux, macOS, and Windows daemon/CLI archives, smoke-test native binaries,
generate checksums, and keep README and architecture release target language in
sync. GLIBC baseline and mount validation remain Linux-only.

The v0.12.1 platform parity hardening validation checks Windows private-file
diagnostic warnings, Windows exec cancellation guarantee wording, cross-platform
`portable-pty` smoke coverage, and platform caveats in `operon doctor`.

The v0.12.2 maintainability cleanup validation checks that daemon gRPC runtime
routing lives in a focused runtime module, CLI exec argv/shell argument helpers
and PTY session UI live in focused command modules, and behavior-preserving
tests still pass.

The v0.12.3 Windows exec process-tree cancellation validation checks the
Windows Job Object integration, platform diagnostics, Windows CI coverage, and
protocol documentation for the current cancellation guarantee.

The v0.12.4 release artifact verification validation checks the public release
asset verifier, the manual GitHub Actions workflow, expected artifact names,
checksum validation, and release smoke command coverage.

The v0.12.5 CLI gRPC maintainability validation checks that the CLI gRPC
compatibility surface delegates filesystem, exec, service, and audit helpers to
focused modules while preserving behavior-covered tests.

The Docker validation starts two reachable `operond` nodes, exercises
capabilities through the CLI, checks auth, policy, audit filters, store
queries, secret use, service health checks, streaming fs, exec stdin/log streams,
LAN mDNS discovery, and runs the example execution graph over gRPC endpoints.
The Linux mount validation adds a real FUSE mount read check when the host has
`/dev/fuse`; otherwise it reports the missing host requirement and exits
cleanly.

The v0.6.1 Linux write mount validation checks create, write, read-after-write,
truncate, delete, rename, denied write/delete/rename audit, and cleanup.

The v0.6.2 CLI fs cleanup validation checks direct CLI mutation commands for
mkdir, truncate, rename, rm, denied mutations, and audit.

The v0.6.3 fs copy validation checks same-node daemon-side copy, denied copy,
and CLI/SDK/protocol copy behavior.

The v0.6.4 onboard validation checks generated unified config, token auth,
daemon startup, CLI ping, capability inspection, fs operation, and audit.

The v0.6.7/v0.6.8/v0.6.12 runtime validation checks process-group
cancellation, binary-safe exec logs, streaming file writes, exec stdin streaming,
and current paginated list API callers.

The v0.6.9 CLI contract validation checks script-facing JSON output, quiet
output, exec failure exit status, audit JSON filters, health version reporting,
and starter config file generation.

The v0.6.12 runtime-boundary validation checks exec-log streaming envelopes,
append-only store writer APIs, Linux-only mount adapter dependency boundaries,
and the current public protocol version.

The v0.7 service forwarding validation checks policy-configured service
metadata, TCP health checks, explicit local port forwarding, and service
forwarding audit events.

The v0.7.1 UDP datagram forwarding validation checks policy-configured UDP
service metadata, local UDP forwarding, packet-boundary preservation, and audit
events.

The v0.8 agent skills validation checks repo-local skill metadata, public CLI
help paths, `operon config explain`, current service forwarding command names,
and safety guidance for agent workflows.

The v0.8.1 integration coverage validation starts a real daemon and exercises
config, node, capability, filesystem, exec, service, audit, execution graph,
trace, and completion flows. The current coverage audit is in
`docs/quality/test-coverage-audit.md`.

The v0.8.3 read-range validation checks the `ReadFileRange` protocol path,
Linux mount random-read behavior, SDK helper coverage, and release/protocol
version policy documentation.

The v0.8.4 modularization validation checks that fs runtime handlers,
pagination helpers, CLI fs command handlers, output rendering helpers, and
target parsing live outside the entrypoint files.

The v0.8.5 core-domain validation checks that [`operon-core`](crates/operon-core) domain DTOs and
policies live in focused modules with compatible root re-exports.

The v0.8.6 modularization validation checks shared Rust gRPC client helpers,
non-fs CLI command modules, Linux mount adapter modules, daemon exec/service/audit
modules, graph/workflow aliases, and TypeScript SDK public API alignment.

The docs/help/skills synchronization validation checks current public CLI help
paths, repo-local skill guidance, AGENTS.md synchronization rules, and stale
provider command examples in docs and skills.

The v0.9 endpoint-model validation checks endpoint-only config, stale-field
warnings, mDNS endpoint candidates, endpoint-only discovery export, and the
absence of automatic capability grants for discovered nodes.

The post-v0.9 discovery UX validation checks mDNS export conflict handling,
optional endpoint health status output, and continued endpoint-only discovery
config generation.

The policy-derived capability validation checks that daemon capability
discovery reflects configured policy mounts, exec roots, and services instead of
advertising a static default capability set.

The v0.9.3 store-backed audit validation checks that persisted audit events are
loaded from the append-only JSONL store at daemon startup while keeping bounded
in-memory retention.

The v0.9.4 runtime hardening validation checks service health audit semantics,
store-backed exec log restart visibility, workspace traversal hardening,
shell-free argv execution, config LAN advertisement UX, and protocol
version alignment.

The v0.9.5 policy language validation checks the shared policy decision
vocabulary, stable deny reason codes, effective `config explain` grants, and
policy audit denial reasons.

The v0.9.6 capability diagnostics validation checks the `ExplainCapability`
runtime RPC, `operon capability explain`, TypeScript SDK helper coverage, and
current protocol version alignment.

The v0.9.7 runtime API hardening validation is covered by the workspace Rust and
TypeScript checks. It verifies paginated `ListFs` protocol behavior,
complete-list CLI/mount/SDK helpers, SDK streaming writes without full
pre-buffering, empty daemon exec request rejection, and runtime API docs for
bidirectional service tunnel RPCs.

The v0.10 exec unification validation checks that the active protocol, CLI,
SDK, docs, examples, validation scripts, and repo-local skills use the unified
`exec` capability vocabulary and that the legacy job command group is not
supported.

The v0.10.1 filesystem consistency validation checks opaque filesystem versions,
mutation preconditions, guarded CLI/SDK writes, and Linux
`openat2(RESOLVE_BENEATH)` workspace hardening.

The v0.10.2 operator diagnostics validation checks `operon doctor`, JSON config
diagnostics, endpoint/auth/protocol/capability/service diagnostics, and docs
coverage.

The v0.11 exec session validation checks the PTY-backed `OpenExecSession`
protocol, `operon exec session` CLI command, SDK helper, policy diagnostics,
and a real local daemon session.

The v0.10.4 maintainability validation checks that exec RPC routing and
PTY/session runtime behavior live behind focused daemon and CLI module
boundaries.

The v0.11.2 exec session hardening validation checks local terminal dimension
defaults, Unix resize forwarding, daemon response-stream drop termination, and
the documented cross-platform `portable-pty` follow-up assessment.

The v0.10.5 maintainability validation checks that service tunnel state
machines and CLI service forwarding transport helpers live behind focused
daemon and CLI module boundaries.

The v0.11.3 platform capability matrix validation checks the macOS/Windows core
runtime capability matrix, platform smoke CI entries, Linux-only mount boundary,
`portable-pty` session direction, and platform-specific shell defaults for
command-string exec/session requests.

## Release Automation

Pushing a tag that matches `v*` starts the `Draft Release` GitHub Actions
workflow:

```bash
git tag v0.x.y
git push origin v0.x.y
```

The workflow creates a draft GitHub Release with Linux `x86_64`, `arm64`, and
`armv7` binary tarballs, macOS `x86_64` and `aarch64` binary tarballs, a
Windows `x86_64` binary zip, a JavaScript SDK tarball, and `SHA256SUMS`. Draft
releases are intentionally left unpublished for manual review.

After publishing a release, verify the public assets from the release download
surface:

```bash
scripts/verify-release-artifacts.sh <tag>
```

The same verification can be started from GitHub Actions with the
`Verify Release Artifacts` workflow. It downloads the public Release assets,
checks `SHA256SUMS`, verifies the expected Linux/macOS/Windows/SDK asset set,
and smoke-tests the current platform's extracted `operon` and `operond`
binaries.

Version policy:

- GitHub release tags identify shipped binary bundles.
- Rust crate versions, the TypeScript SDK package version, and
  `PROTOCOL_VERSION` must align with the public release tag before publishing a
  release so `operon --version`, `operond --version`, SDK package metadata, and
  `operon node ping` / health output do not disagree.
- For internal or unpublished development work, defer the version bump until the
  release preparation commit.

## Onboarding Details

`operon onboard` is a guided first-run setup helper. It writes a unified
`config.yaml` plus referenced secret files where needed.

```bash
operon onboard
```

For reproducible setup, use non-interactive mode:

```bash
operon onboard \
  --yes \
  --role both \
  --output-dir .operon \
  --node-id local \
  --workspace /workspace \
  --listen 0.0.0.0:7789
```

This writes:

```text
.operon/config.yaml
.operon/token
.operon/daemon-command.txt
```

`config.yaml` is the runtime config entrypoint for both `operon` and `operond`.
If `--config` is omitted, both binaries read `$HOME/.operon/config.yaml`.

## Unified Config Reference

Operon uses one YAML config schema with separate daemon, client, policy, auth,
store, and secret-reference sections:

```yaml
version: 1

daemon:
  node_id: cloud-a
  grpc_listen: 0.0.0.0:7789
  workspace: /home/ubuntu/workspace
  advertise_lan: true
  store: store.jsonl
  auth:
    token_file: token

client:
  nodes:
    cloud-a:
      endpoint: grpc://100.96.12.34:7789
      auth:
        token_file: token
    gpu-node:
      endpoint: grpc://100.96.18.20:7789

policy:
  subject: local-cli
  fs:
    mounts:
      - name: workspace
        path: /
        permissions:
          read: true
          write: true
          delete: false
  exec:
    allowed_cwds:
      - /
    default_timeout_secs: 30
    max_timeout_secs: 300
    allow_sessions: true
    preserve_env: false
    env_allowlist: []
    allowed_secrets: []
  service:
    services:
      - id: daemon
        name: daemon
        host: 127.0.0.1
        port: 7789
        protocol: tcp
        description: Operon gRPC daemon listener

secrets:
  file: secrets.yaml
```

`endpoint` may be `grpc://` or `grpcs://`. The CLI uses gRPC for runtime
operations. Use `operon --json` for scripts, and use `PROTOCOL.md` if you need
to integrate without an SDK. Auth can use `token`, `token_file`, or
`token_env`; file paths are resolved relative to the config file directory.

Cloudflare Mesh, Tailscale, WireGuard, Kubernetes DNS, LAN IPs, manual DNS
names, and trusted tunnel endpoints are all ordinary endpoints to Operon. LAN
mDNS discovery can find local Operon daemons, but Operon still does not create
VPNs, assign mesh IPs, or grant capability access through discovery.

Discovery export is intentionally conservative. `operon node discover
--output-config <path>` writes endpoint-only client nodes. If `<path>` already
exists, newly discovered nodes are merged into it, but a discovered node id that
points at a different existing endpoint is rejected instead of overwritten. Use
`operon node discover --check-health` when you want best-effort runtime health
status for discovered endpoint candidates before importing them.

External control-plane scripts can generate the same endpoint-only YAML shape
from Cloudflare, Tailscale, Kubernetes, inventory databases, or DNS. Those
scripts should write `client.nodes.<node_id>.endpoint` entries and leave
capability policy changes to normal Operon configuration review.

## Policy Reference

Policy shape. In config paths and generated config diagnostics, this execution
policy is referred to as `policy.exec`:

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

exec:
  allowed_cwds:
    - /
  default_timeout_secs: 30
  max_timeout_secs: 300
  allow_sessions: true
  preserve_env: false
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
      permissions:
        check: true
        forward: true
```

Policy paths are virtual paths inside the daemon workspace. If the daemon
config sets `workspace: /home/ubuntu/workspace`, policy path `/` means that
workspace root, not the host root.

`preserve_env: false` keeps exec process environments isolated. With this
default, the daemon clears inherited environment variables and injects only
`env_allowlist` variables plus authorized requested secrets. Set
`preserve_env: true` only when execs need the full daemon environment, including
values such as `HOME`, `PATH`, proxy settings, or toolchain variables.

Secret file shape:

```yaml
GITHUB_TOKEN: ghp_example
```

Secrets are only injected into execs that request them and are allowed by policy.
The daemon does not expose a secret read API; audit output records secret names,
not values.

Policy decisions use a small shared policy vocabulary across filesystem, exec,
service, and secret checks. Denials carry a stable reason code such as
`fs-mount-not-allowed`, `fs-permission-denied`, `exec-cwd-denied`,
`exec-timeout-exceeded`, `secret-denied`, `secret-undefined`,
`service-unknown`, or `service-action-denied`, followed by a human-readable
message in audit output. `operon config explain --json` includes
`policy.effective_grants` entries with `capability_id`, `action`, `resource`,
`allowed`, and `reason_code` fields so agents can inspect the effective policy
surface without reading secret values.

Use `operon capability explain <node> <capability_id> <action> <resource>` to
ask the target daemon why one capability action is allowed or denied. The JSON
form returns the same `PolicyDecision` fields used internally by the daemon:
`subject`, `capability_id`, `action`, `resource`, `allowed`, `reason_code`, and
`message`.

## Command Reference

```bash
operon --config ./operon.config.yaml node list
operon --config ./operon.config.yaml node resolve cloud-a
operon node discover --timeout-secs 3
operon node discover --timeout-secs 3 --check-health
operon --config ./operon.config.yaml node ping cloud-a
operon --config ./operon.config.yaml capability list cloud-a
operon --config ./operon.config.yaml capability explain cloud-a fs:workspace read /
operon --config ./operon.config.yaml service list cloud-a
operon --config ./operon.config.yaml service check cloud-a daemon
operon --config ./operon.config.yaml service forward cloud-a web --listen 127.0.0.1:8080
operon --config ./operon.config.yaml service forward-udp cloud-a dns --listen 127.0.0.1:5353

operon init config ./operon.config.yaml

operon --config ./operon.config.yaml fs stat cloud-a:/README.md
operon --config ./operon.config.yaml fs list cloud-a:/
operon --config ./operon.config.yaml fs read cloud-a:/input.txt
operon --config ./operon.config.yaml fs read cloud-a:/large.bin --output ./large.bin
operon --config ./operon.config.yaml fs write cloud-a:/input.txt --content "hello"
operon --config ./operon.config.yaml fs write cloud-a:/large.bin --file ./large.bin
operon --config ./operon.config.yaml fs mkdir cloud-a:/work
operon --config ./operon.config.yaml fs truncate cloud-a:/work/file.txt --size 0
operon --config ./operon.config.yaml fs rename cloud-a:/work/file.txt cloud-a:/work/renamed.txt
operon --config ./operon.config.yaml fs copy cloud-a:/work/renamed.txt cloud-a:/work/copied.txt
operon --config ./operon.config.yaml fs rm cloud-a:/work/renamed.txt

operon --config ./operon.config.yaml exec run cloud-a -- echo hello
operon --config ./operon.config.yaml exec run cloud-a --argv -- printf "hello world"
operon --config ./operon.config.yaml exec run cloud-a --secret GITHUB_TOKEN -- 'test x$GITHUB_TOKEN = xexpected'
operon --config ./operon.config.yaml exec run cloud-a --detach -- sleep 10
operon --config ./operon.config.yaml exec status cloud-a exec-1
operon --config ./operon.config.yaml exec list cloud-a
operon --config ./operon.config.yaml exec logs cloud-a exec-1
operon --config ./operon.config.yaml exec logs cloud-a exec-1 --follow
operon --config ./operon.config.yaml exec logs cloud-a exec-1 --stream
operon --config ./operon.config.yaml exec stdin cloud-a exec-1 --content "input"
operon --config ./operon.config.yaml exec stdin cloud-a exec-1 --close
operon --config ./operon.config.yaml exec cancel cloud-a exec-1

operon --config ./operon.config.yaml audit list cloud-a
operon --config ./operon.config.yaml audit show cloud-a --limit 20
operon --config ./operon.config.yaml audit show cloud-a --capability service:daemon --action check --allowed true --resource daemon --limit 5
operon --config ./operon.config.yaml run --trace-output ./trace.json examples/train-model.yaml
operon trace list .
operon trace show ./trace.json
operon --json trace show ./trace.json
operon --config ./operon.config.yaml mount cloud-a:/ --to ./cloud-a
```

When `daemon.store` is configured, audit events are appended to the JSONL store
and loaded again on daemon startup. `audit list` and `audit show` still read
from the bounded in-memory audit queue, seeded from the most recent persisted
events.

`operon exec run` treats one argument after `--` as an explicit shell command
string. Multiple arguments are shell-escaped before being sent to the daemon so
argument boundaries are preserved. For shell operators, expansion, or pipelines,
pass one quoted command string or call `sh -c`.

Use `operon exec run --argv -- <program> <arg>...` to send a shell-free argv
request; this preserves arguments without shell parsing and is preferred for
agents when no shell syntax is needed.

Exec stdout/stderr logs are transported as bytes. Human CLI output writes those
bytes directly; JSON output exposes byte arrays so clients can choose their own
decoding.

Add `--json` for structured command output or `--quiet` to suppress
non-essential output.

## Linux Mount Notes

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

## Protocol Integration Notes

The current CLI speaks gRPC to `grpc://` daemon endpoints. There is no direct
HTTP runtime API; humans and scripts should use `operon`, including
`operon --json`, and programs should use SDKs or generated clients from
[`proto/operon/runtime.proto`](proto/operon/runtime.proto).

The runtime schema uses typed protobuf enums, proto3 optional presence,
target/chunk request envelopes, exec-log stream event envelopes, bidirectional
service tunnel RPCs, and paginated list APIs.

See [PROTOCOL.md](PROTOCOL.md) and
[docs/architecture/runtime-api.md](docs/architecture/runtime-api.md) for direct
protocol integration details.

## TypeScript SDK Example

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
    { node: "gpu-node", action: "exec.run", command: "train.py", secrets: ["WANDB_API_KEY"] }
  ]
});
```

## Developer Examples

Run the local Docker gRPC demo:

```bash
scripts/verify-v0.5-docker.sh
```

This starts two `operond` containers with gRPC listeners, validates capability
discovery, token auth, fs operations, streaming file transfer, command execution,
stdin/log streams, service checks, policy denial, scoped secrets, audit output,
trace summaries, and runs:

```bash
operon --config examples/docker-config.yaml run --trace-output /tmp/operon-docker-grpc-trace.json examples/docker-copy-and-run.yaml
```

Run the Linux FUSE mount validations:

```bash
scripts/verify-v0.6-linux-mount.sh
scripts/verify-v0.6.1-linux-write-mount.sh
scripts/verify-v0.6.2-cli-fs-cleanup.sh
scripts/verify-v0.6.3-fs-copy.sh
scripts/verify-v0.6.4-onboard.sh
scripts/verify-v0.6.7-runtime.sh
```

## Architecture Notes

```text
AI Agent / CLI / SDK
        |
Operon Runtime
  - Operon Graph
  - Scheduler
  - Execution Trace
        |
Capability Layer
  - fs / exec / service / secret
        |
Policy / Secret / Audit
        |
Agent API
  - gRPC
        |
Configured Endpoint
  - grpc:// or grpcs:// over an existing private network
        |
Existing Secure Private Network
```

## Phase Tracking

`docs/plan/development-phases.md` is the authoritative phase tracker for
planned and completed development work. Update it when a task advances,
completes, or changes phase scope.
