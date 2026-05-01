# Development Phases

This plan translates the current product and architecture decisions into an implementation sequence.

Related documents:

- `docs/decisions/computer-mesh-operon-summary.md`
- `docs/architecture/technology-and-protocol-decisions.md`
- `README.md`

## MVP Goal

Operon v0.1 should prove this:

```text
Across two machines that are already reachable through Cloudflare Mesh,
Tailscale, WireGuard, SSH, LAN, Kubernetes networking, or manual private
endpoints, users can discover capabilities, run fs/job operations, and
receive a structured execution trace through the CLI/SDK.
```

The MVP is not about building networking infrastructure. It is about proving the capability runtime.

## MVP Non-goals

Do not build these in v0.1:

- VPN
- NAT traversal
- relay network
- device mesh IP assignment
- global routing
- automatic network discovery
- FUSE / WinFsp mount layer
- screen streaming
- audio
- clipboard sync
- full file synchronization engine
- Web Console
- plugin system
- complex policy language
- full secret manager

## Phase 0: Foundation

Status: Completed.

Goal: make the repository a sustainable engineering base.

Current status: completed on 2026-04-30.

Completed:

- Rust workspace initialized.
- TypeScript workspace initialized.
- Initial protobuf schema added.
- `operond` daemon crate added.
- `operon` CLI crate added.
- capability crates added.
- examples added.
- architecture and decision docs added.
- `cargo check --workspace` passed.
- `cargo fmt --check` passed.
- `cargo clippy --workspace --locked -- -D warnings` passed.
- TypeScript dependency lockfile added.
- `pnpm typecheck` passed for workspace packages.
- GitHub Actions CI added for Rust checks and TypeScript typecheck.

Remaining:

- None for Phase 0.

Deliverables:

- Rust workspace
- TypeScript workspace
- initial protobuf schema
- `operond` daemon crate
- `operon` CLI crate
- capability crates
- examples
- architecture and decision docs
- CI for Rust checks

Validation:

```bash
cargo check --workspace
cargo fmt --check
cargo clippy --workspace
```

Done when:

- workspace compiles
- CI runs basic checks
- README and decision docs agree on project scope

## Phase 1: Node Runtime and Manual Network

Status: Completed.

Goal: run real daemons on already-reachable nodes.

Current status: completed on 2026-04-30 with Docker two-node validation.

Completed:

- `operond start --listen <addr> --node-id <id>` implemented.
- daemon exposes `GET /health`.
- daemon exposes `GET /node`.
- manual YAML endpoint config is loadable.
- `operon node list` implemented.
- `operon node ping <node-id>` implemented for Phase 1 `http://` endpoints.
- `examples/nodes.yaml` includes a local endpoint.
- local validation passed with `operond` on `127.0.0.1:7788` and `operon node ping local`.
- Docker two-node validation added through `docker-compose.yml`, `docker/Dockerfile`, `examples/docker-nodes.yaml`, and `scripts/verify-mvp-docker.sh`.
- Docker two-node validation passed with `node-a` and `node-b`.

Remaining:

- None for Phase 1.

Commands:

```bash
operond start --listen 0.0.0.0:7788
operon node list
operon node ping cloud-a
```

Configuration:

```yaml
nodes:
  local:
    endpoint: http://127.0.0.1:7788
  cloud-a:
    endpoint: http://100.96.12.34:7788
```

Modules:

- `operond`: daemon lifecycle
- `operon-network`: manual endpoint resolver
- `operon-core`: shared health and node info types
- `operon-cli`: node commands

Implementation notes:

- assume TCP reachability
- do not auto-discover nodes
- do not provision network connectivity
- separate node identity from network endpoint

Done when:

- Docker two-node validation passes locally or in CI
- CLI can ping a remote daemon
- CLI can show remote node identity and basic metadata

Notes:

- Real private-network two-machine validation is not required for Phase 1; Docker two-node validation is sufficient because Operon intentionally treats network connectivity as an external dependency.
- HTTPS support is deferred until identity/auth requirements are designed.
- Generated gRPC or a formal HTTP client can replace the temporary hand-written HTTP client in a later protocol phase.

## Phase 2: Capability Discovery

Status: Completed.

Goal: nodes can declare machine-readable capabilities.

Current status: completed on 2026-04-30 with Docker two-node validation.

Completed:

- shared `Capability`, `CapabilityKind`, and `CapabilityList` types added.
- daemon exposes `GET /capabilities`.
- default Phase 2 capabilities are advertised: `fs`, `process`, `job`, `device-info`, and `service`.
- `operon capability list <node-id>` implemented.
- Docker two-node validation passed for `node-a` and `node-b` capability discovery.
- `scripts/verify-mvp-docker.sh` now validates both node health and capability discovery.

Remaining:

- None for Phase 2.

Initial capability kinds:

```text
fs
process
job
device-info
service
```

Command:

```bash
operon capability list cloud-a
```

Example output:

```text
cloud-a/fs:workspace read,write
cloud-a/process:default run
cloud-a/job:default run,cancel,logs
cloud-a/device-info:default read
```

Capability metadata should include:

- id
- kind
- node id
- exposed name
- permissions
- resource constraints
- human-readable description

Done when:

- daemon exposes capability list over protocol
- CLI renders capabilities
- capability IDs can be referenced by later execution steps

## Phase 3: Filesystem Capability

Status: Completed.

Goal: make remote filesystem operations work through protocol calls, without mount support.

Current status: completed on 2026-04-30 with Docker two-node validation.

Completed:

- daemon exposes `GET /fs/stat`.
- daemon exposes `GET /fs/list`.
- daemon exposes `GET /fs/read`.
- daemon exposes `POST /fs/write`.
- daemon constrains all fs operations to the configured workspace mount.
- CLI implements `operon fs stat <node:/path>`.
- CLI implements `operon fs list <node:/path>`.
- CLI implements `operon fs read <node:/path>`.
- CLI implements `operon fs write <node:/path> --content <text>`.
- Docker image creates a writable `/workspace` mount.
- minimal audit log added through `GET /audit`.
- CLI implements `operon audit list <node-id>`.
- Docker validation passes fs stat/list/read/write on `node-a` and `node-b`.
- Docker validation confirms path escape attempts are denied and recorded in audit output.

Remaining:

- None for Phase 3.

Commands:

```bash
operon fs stat cloud-a:/workspace/README.md
operon fs list cloud-a:/workspace
operon fs read cloud-a:/workspace/a.txt
operon fs write cloud-a:/workspace/a.txt --content "hello"
```

MVP operations:

- stat
- list
- read stream
- write stream

Deferred operations:

- delete
- watch
- sync
- mount
- cache coherence

Requirements:

- paths must be constrained by configured mounts
- arbitrary host root access must be impossible
- read/write permissions must be enforced
- large files must stream instead of buffering fully
- every operation must emit audit events

Done when:

- a remote node exposes one configured directory
- CLI can stat/list/read/write inside that directory
- path traversal outside the mount is rejected
- audit records show allowed and denied fs operations

## Phase 4: Process and Job Capability

Status: Completed.

Goal: run controlled commands remotely and stream their lifecycle.

Current status: completed on 2026-05-01 with Docker two-node validation.

Completed:

- daemon exposes `POST /job/run`.
- daemon exposes `GET /job/status`.
- daemon exposes `GET /job/logs`.
- daemon exposes `POST /job/cancel`.
- daemon keeps an in-memory job table with lifecycle status and exit code.
- daemon captures stdout/stderr as structured `JobLog` entries.
- daemon supports timeout and best-effort cancellation.
- daemon constrains job cwd to the configured workspace mount.
- job run and cancel operations emit audit records.
- CLI implements `operon job run <node-id> -- <command>`.
- CLI implements `operon job run <node-id> --detach -- <command>`.
- CLI implements `operon job status <node-id> <job-id>`.
- CLI implements `operon job logs <node-id> <job-id>`.
- CLI implements `operon job cancel <node-id> <job-id>`.
- Docker validation passes job run/log/status/cancel/timeout on `node-a` and `node-b`.

Remaining:

- None for Phase 4.

Commands:

```bash
operon job run cloud-a -- echo hello
operon job run gpu-node --cwd /workspace -- python train.py
operon job logs cloud-a <job-id>
operon job status cloud-a <job-id>
operon job cancel cloud-a <job-id>
```

MVP features:

- run command
- cwd allowlist
- env allowlist
- stdout/stderr streaming
- exit code
- timeout
- cancel
- job record

Deferred features:

- PTY
- interactive shell
- container sandbox
- resource limits
- secret injection

Requirements:

- jobs must be bounded by policy
- stdout/stderr must stream as events
- exit status must be queryable after completion
- cancellation must be best effort but observable

Done when:

- remote command execution works
- logs stream live
- completed jobs can be queried
- cancelled and timed-out jobs are represented correctly

## Phase 5: Operon Execution Graph

Status: Completed.

Goal: compose capability calls into a traceable execution unit.

Current status: completed on 2026-05-01 with Docker two-node validation.

Completed:

- shared `ExecutionGraph`, `ExecutionStep`, `ExecutionTrace`, and trace status types added.
- CLI implements `operon run <workflow.yaml>`.
- workflow YAML executes steps sequentially.
- supported graph actions: `fs.stat`, `fs.list`, `fs.read`, `fs.write`, and `job.run`.
- each step records id, node, action, status, start/end timestamps, error, and output.
- graph execution stops on the first failed step and prints the partial trace.
- graph execution prints structured JSON trace for human and agent consumption.
- example Docker workflow added at `examples/docker-copy-and-run.yaml`.
- `examples/train-model.yaml` updated with explicit step ids and write content.
- Docker validation passes the copy/run/read graph demo on `node-a`.

Remaining:

- None for Phase 5.

Example:

```yaml
name: train-model

steps:
  - id: read-data
    node: nas
    action: fs.read
    path: /data/images

  - id: train
    node: gpu-node
    action: job.run
    command: python train.py

  - id: save-model
    node: cloud-a
    action: fs.write
    path: /models/output
```

Commands:

```bash
operon run examples/train-model.yaml
```

MVP graph fields:

- run id
- step id
- node
- action
- status
- started_at
- ended_at
- logs
- error
- output/artifact metadata

Scheduling:

- v0.1 should execute steps sequentially
- no complex DAG scheduler in MVP

Deferred:

- daemon-persisted traces
- `operon trace show <run-id>`
- parallel DAG scheduling
- artifact store
- retry policy

Done when:

- YAML steps execute in order
- each step has structured status
- failure identifies the failed step and reason
- trace can be inspected after execution

## Phase 6: Minimal Policy and Audit

Status: Completed.

Goal: make capability use explicit, scoped, and traceable.

Current status: completed on 2026-05-01 with Docker two-node validation.

Completed:

- shared policy types added for fs mounts, fs permissions, job cwd allowlist, job timeout limits, and env allowlist.
- daemon supports `operond start --policy <policy.yaml>`.
- daemon keeps a default policy when no policy file is provided.
- fs stat/list/read/write are checked against configured fs mount permissions before execution.
- job run checks cwd allowlist and timeout maximum before execution.
- Docker nodes load `examples/docker-policy.yaml`.
- audit records now include subject, timestamp, run id placeholder, and step id placeholder.
- CLI audit output renders the expanded audit fields.
- Docker validation confirms path escape denial, job timeout policy denial, allowed operations, and denied operations are all recorded in audit output.

Remaining:

- None for Phase 6.

Example node policy:

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

MVP policy:

- fs mount allowlist
- fs read/write/delete permission flags
- job cwd allowlist
- job timeout
- env allowlist
- subject node id

Audit event fields:

- subject
- timestamp
- node
- capability
- action
- resource
- allowed or denied
- policy reason
- run id
- step id

Done when:

- denied operations are blocked
- allowed and denied operations create audit records
- traces can link to audit events
- policy is simple enough to reason about in examples

## Phase 7: Minimal SDK and Demo Packaging

Status: Completed.

Goal: expose the MVP through an agent-friendly SDK and a runnable demo.

Current status: completed on 2026-05-01 with Docker two-node validation.

Completed:

- TypeScript SDK now exposes `OperonClient`.
- SDK can run sequential workflows over configured node endpoints.
- SDK supports `fs.stat`, `fs.list`, `fs.read`, `fs.write`, and `job.run`.
- SDK returns a structured trace with run id, step status, timing, error, and output fields.
- README now documents the Docker MVP demo command.
- README now shows the runnable `examples/docker-copy-and-run.yaml` workflow.
- README status checklist reflects the completed MVP runtime pieces.
- Docker validation remains the reproducible fresh-checkout demo path.

Remaining:

- None for Phase 7.

TypeScript SDK shape:

```ts
import { OperonClient } from "@operon/sdk";

const operon = new OperonClient([
  { nodeId: "cloud-a", endpoint: "http://100.96.12.34:7788" },
  { nodeId: "gpu-node", endpoint: "http://100.96.18.20:7788" }
]);

const trace = await operon.run({
  name: "train-model",
  steps: [
    { node: "cloud-a", action: "fs.read", path: "/workspace/a.txt" },
    { node: "gpu-node", action: "job.run", command: "python train.py" }
  ]
});
```

SDK should call the local daemon HTTP facade, not require consumers to speak gRPC directly.

Required demo:

```text
two machines:
  local
  cloud-a

workflow:
  1. write a file to cloud-a workspace
  2. run a command on cloud-a
  3. read the output
  4. show the execution trace
```

Demo command:

```bash
scripts/verify-mvp-docker.sh
```

Done when:

- README demo can be run from a fresh checkout
- SDK can submit a run request
- trace output is useful for humans and agents

## Phase 8: MVP Release Baseline

Status: Completed.

Goal: make v0.1.0 explicit, reproducible, and ready to tag.

Current status: completed on 2026-05-01 with local validation and CI workflow updates.

Completed:

- Docker MVP validation script renamed to `scripts/verify-mvp-docker.sh`.
- README Quickstart added with the full MVP validation command set.
- README demo command updated to use the MVP validation script.
- `docs/plan/mvp-acceptance.md` added as the v0.1.0 acceptance baseline.
- MVP acceptance document records scope, non-goals, validation commands, release checklist, and known limitations.
- CI now runs on pushes to both `main` and `mvp`.
- CI now includes an `MVP Docker Validation` job that runs `scripts/verify-mvp-docker.sh` after Rust and TypeScript checks.
- Old `verify-phase1-docker.sh` references were updated to the MVP script name.
- Unit test baseline added across core Rust modules and the TypeScript SDK.
- CI Node setup updated to `actions/setup-node@v6`.

Remaining:

- None for Phase 8.

Validation:

```bash
cargo fmt --check
cargo test --workspace --locked
cargo check --workspace --locked
cargo clippy --workspace --locked -- -D warnings
pnpm -r test
pnpm typecheck
scripts/verify-mvp-docker.sh
```

Done when:

- MVP validation has one canonical command.
- README Quickstart matches the canonical command.
- v0.1.0 acceptance criteria are documented.
- CI covers Rust, TypeScript, and Docker MVP validation.

## MVP Definition of Done

v0.1 is complete when:

- `operond` runs on at least two machines
- nodes are connected through manually configured reachable endpoints
- CLI can discover remote capabilities
- fs stat/list/read/write work through configured mounts
- job run/logs/status/cancel work for controlled commands
- `operon run` executes sequential YAML steps
- every run produces a structured trace
- basic policy blocks out-of-scope fs/job operations
- audit records capture allowed and denied actions
- README demo can be reproduced

## Post-MVP Phases

### v0.2: Provider Adapters and Secrets

- Cloudflare Mesh endpoint adapter
- Tailscale endpoint adapter
- SSH endpoint adapter
- SecretCapability MVP
- stronger node identity and trust establishment
- better error model

### v0.3: Mounts and Developer Experience

- FUSE / WinFsp mount adapter
- richer CLI output
- local caching strategy
- LAN mDNS discovery
- Kubernetes service discovery
- improved examples

### v0.4: Web Console and Advanced Capabilities

- Web Console
- screen/input capability
- clipboard capability
- service/port access capability
- richer policy language
- trace visualization

## Planning Principle

Every phase should preserve the core boundary:

```text
Cloudflare Mesh / Tailscale / WireGuard / SSH / LAN solve connectivity.
Operon solves what connected machines are allowed to do, how execution is
composed, and how results are traced.
```
