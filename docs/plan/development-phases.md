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
- CI checkout and pnpm setup actions updated to `actions/checkout@v6` and `pnpm/action-setup@v6`.
- Direct Rust and TypeScript dependencies refreshed to current compatible releases where available.
- README CLI and configuration documentation added, including node config path, daemon policy config, and common commands.

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

## v0.2 Goal

Operon v0.2 should turn the v0.1 functional MVP into a more usable runtime:

```text
Remote filesystem and job IO can be consumed incrementally, daemon calls have
predictable errors and minimal authentication, provider resolution is explicit,
audit/job/trace state can survive daemon restarts, and jobs can use scoped
secrets without exposing secret values directly.
```

v0.2 still does not build network connectivity. Provider adapters resolve endpoints only.

## Phase 9: Streaming IO Semantics

Status: Completed.

Goal: make fs/job IO usable for larger files and long-running commands.

Planned:

- fs read streaming endpoint.
- fs write streaming endpoint.
- job stdout/stderr follow endpoint.
- CLI `fs read --output <file>`.
- CLI `fs write --file <file>`.
- CLI `job logs --follow`.
- SDK streaming-friendly helpers.

Completed:

- Added daemon raw-body fs read/write endpoints at `/fs/read-stream` and `/fs/write-stream`.
- Added CLI `fs read --output <file>` and `fs write --file <file>` paths.
- Added CLI `job logs --follow` for long-running job output.
- Added SDK raw fs byte helpers.
- Extended Docker validation to cover raw fs transfer and followed logs.

Remaining:

- True chunked daemon-side fs streaming and job stdin streaming are deferred beyond v0.2.

Done when:

- large fs reads/writes avoid JSON string payloads.
- job logs can be followed while a job is still running.
- Docker validation covers streamed fs read/write and followed job output.

## Phase 10: Structured Errors and Minimal Auth

Status: Completed.

Goal: make daemon failures predictable and prevent unauthenticated daemon access.

Planned:

- shared structured error response type.
- daemon handlers return `{ code, message, capability, resource }`.
- CLI preserves meaningful daemon error messages.
- daemon `--auth-token` and `--auth-token-file`.
- CLI node config supports `token`.
- SDK supports bearer token.
- audit subject comes from request identity when provided.

Completed:

- Added shared structured error response type and daemon JSON error responses.
- Added daemon `--auth-token` and `--auth-token-file`.
- Added per-node CLI config `token` support.
- Added per-node SDK token support.
- Updated CLI HTTP helpers to send bearer tokens and preserve daemon error messages.
- Updated SDK error parsing for structured daemon error bodies.
- Added CLI unit coverage for structured daemon error formatting.
- Extended Docker validation to cover unauthorized and authorized calls.

Remaining:

- Request-derived audit identity remains future work.

Done when:

- unauthorized requests are rejected.
- authorized CLI/SDK calls continue to work.
- denied operations return structured JSON errors.
- Docker validation covers unauthorized and authorized requests.

## Phase 11: Provider Resolver Adapters

Status: Completed.

Goal: make network providers explicit without implementing connectivity.

Planned:

- provider resolver trait in `operon-network`.
- manual resolver implementation.
- provider metadata validation for Cloudflare Mesh, Tailscale, WireGuard, SSH, LAN, and Kubernetes.
- CLI `node resolve <node-id>`.
- CLI `provider list`.

Completed:

- Added explicit provider kinds in `operon-network`.
- Added endpoint resolution through `NodesConfig::resolve`.
- Added CLI `node resolve <node-id>` and `provider list`.
- Extended Docker validation to cover manual provider resolution.

Remaining:

- Provider API discovery for Cloudflare Mesh, Tailscale, LAN mDNS, and Kubernetes is deferred.

Done when:

- config endpoints resolve through provider abstraction.
- unsupported provider values fail clearly.
- Docker validation covers manual provider resolution.

## Phase 12: Persistent Store for Jobs, Audit, and Traces

Status: Completed.

Goal: keep runtime records useful after process-local operations complete.

Planned:

- local JSONL store path.
- daemon `--store <path>`.
- persist audit events.
- persist job records on completion/update.
- CLI `trace show <run-id>` for CLI-generated trace files.
- graph execution can write trace JSON to disk.

Completed:

- Added daemon `--store <path>` JSONL append path.
- Persisted audit events and final job records to the store.
- Added graph `--trace-output <path>`.
- Added CLI `trace show <path>`.
- Extended Docker validation to assert store file creation and trace display.

Remaining:

- The store is append-only JSONL, not a queryable database.
- Daemon-persisted graph traces are deferred.

Done when:

- audit/job records are appended to a local store.
- graph traces can be written and shown.
- Docker validation covers store file creation and trace show.

## Phase 13: Secrets MVP

Status: Completed.

Goal: allow jobs to use scoped secrets without direct secret reads.

Planned:

- local secrets YAML.
- daemon `--secrets <path>`.
- policy secret allowlist.
- `job.run` supports `secrets`.
- daemon injects allowed secrets into job env.
- audit records secret usage by name only.

Completed:

- Added daemon `--secrets <path>` local YAML loading.
- Added policy `job.allowed_secrets`.
- Added `job.run` secret requests and CLI `--secret <NAME>`.
- Injected allowed secrets into job environment only for that job.
- Audited secret usage by name.
- Added daemon unit coverage for allowed and denied secret resolution.
- Extended Docker validation to cover allowed and denied secret requests.

Remaining:

- Full secret manager integration and secret rotation are deferred.

Done when:

- jobs can use allowed secrets as env vars.
- denied secrets are blocked.
- secret values are never returned by API or audit output.

## Phase 14: v0.2 Acceptance

Status: Completed.

Goal: make v0.2 reproducible and documented.

Planned:

- README updates for streaming, auth, providers, store, and secrets.
- `docs/plan/v0.2-acceptance.md`.
- Docker validation covers all v0.2 additions.
- CI remains green.

Completed:

- Updated README for v0.2 CLI, node config, daemon policy, auth, store, provider resolution, secrets, trace, and Docker validation.
- Added `docs/plan/v0.2-acceptance.md`.
- Renamed Docker validation script to `scripts/verify-v0.2-docker.sh`.
- Updated CI to run the v0.2 Docker validation job.

Remaining:

- Final CI status depends on the pushed branch run.

Done when:

- v0.2 has a canonical validation path.
- docs accurately describe the runtime limits and commands.

## v0.3 Goal

Operon v0.3 should turn the v0.2 runtime into a more usable local-network tool:

```text
Core IO is streaming-friendly, CLI output is predictable for humans and agents,
runtime records are queryable, LAN nodes can be discovered through mDNS, and
mount semantics are validated through a narrow proof of concept.
```

v0.3 discovery is LAN mDNS only. Cloudflare, Tailscale, WireGuard, SSH, and Kubernetes remain manually configured endpoint providers unless a later phase explicitly adds API discovery.

## Phase 15: Streaming Protocol Hardening

Status: Completed.

Goal: remove v0.2's most important IO limits.

Planned:

- daemon fs read endpoint streams file bytes without loading the whole file into memory.
- daemon fs write endpoint consumes request bodies incrementally.
- job stdout/stderr streaming endpoint for live consumers.
- job stdin write endpoint for interactive or pipe-like jobs.
- CLI and SDK helpers for streaming fs and job IO.

Completed:

- Daemon `/fs/read-stream` now streams file bytes through `ReaderStream`.
- Daemon `/fs/write-stream` consumes request body chunks incrementally.
- Added `/job/logs-stream`, `/job/stdin`, and `/job/stdin/close`.
- CLI writes file uploads from disk without preloading the whole file and can stream downloads to a writer.
- CLI supports `job logs --stream` and `job stdin`.
- SDK exposes raw fs helpers, job log stream, job stdin write/close, and job listing.

Remaining:

- gRPC streaming and full TTY-style job sessions remain post-v0.3.

Done when:

- large fs transfers are streamed server-side.
- job output can be consumed from a streaming endpoint.
- stdin can be written to a running job.
- Docker validation covers fs streaming and job stdin/stdout streaming.

## Phase 16: CLI UX and Output Modes

Status: Completed.

Goal: make CLI output predictable for both humans and automation.

Planned:

- global `--json`.
- global `--quiet`.
- clearer structured error display.
- `operon init config`.
- `operon init policy`.

Completed:

- Added global `--json`.
- Added global `--quiet`.
- Added structured JSON output for core node, provider, capability, fs, audit, and job commands.
- Added `operon init config <path>`.
- Added `operon init policy <path>`.

Remaining:

- Rich table formatting and shell completions remain future work.

Done when:

- core commands can produce JSON output.
- quiet mode suppresses non-essential output.
- init commands generate usable starter files.
- README documents output modes and init commands.

## Phase 17: Queryable Runtime Store

Status: Completed.

Goal: make audit, job, and trace records inspectable after execution.

Planned:

- daemon reloads job records from the JSONL store on startup.
- daemon exposes job list query.
- CLI `job list`.
- CLI `audit show`.
- CLI `trace list`.
- keep JSONL for v0.3 unless implementation proves it insufficient.

Completed:

- Daemon reloads completed jobs from the JSONL store on startup.
- Added `/job/list`.
- Added CLI `job list`.
- Added CLI `audit show --limit`.
- Added CLI `trace list`.
- Kept JSONL as the v0.3 store format.

Remaining:

- Indexed store queries and SQLite remain future decisions.

Done when:

- completed jobs can be listed through the daemon.
- audit records have a clearer show path.
- local trace files can be listed and shown.
- Docker validation covers store-backed job listing.

## Phase 18: LAN mDNS Discovery

Status: Completed.

Goal: discover Operon daemons on the local network without owning connectivity.

Planned:

- daemon can advertise node id, provider, endpoint, and capability summary through LAN mDNS.
- CLI `node discover --provider lan`.
- discovered records are displayed and can optionally be written into a node config file.

Completed:

- Added daemon `--advertise-lan` mDNS advertisement.
- Added CLI `node discover --provider lan`.
- Added optional `--output-config` for discovered node config generation.
- Docker validation runs LAN discovery inside the compose network.

Remaining:

- Discovery for Cloudflare, Tailscale, WireGuard, SSH, and Kubernetes is intentionally not implemented in v0.3.

Done when:

- LAN mDNS discovery works between local Docker nodes or host processes.
- discovery results are endpoints only.
- docs explicitly state that discovery does not grant capability access.

## Phase 19: Mount PoC

Status: Completed.

Goal: validate mount semantics before committing to a full FUSE / WinFsp layer.

Planned:

- read-only mount proof of concept.
- document path mapping, cache, permission, and error semantics.
- keep WinFsp implementation deferred.

Completed:

- Added CLI `mount read-only <node:/path> --to <dir>` as a one-shot read-only materialization PoC.
- The PoC writes `.operon-mount.json` documenting source, mode, cache, and consistency semantics.
- Files are marked read-only after materialization.

Remaining:

- FUSE / WinFsp live mounts and sync are deferred.

Done when:

- a narrow read-only mount or mount-like PoC exists.
- known semantic risks are documented.
- v0.4 can decide whether to implement a full mount layer.

## Phase 20: v0.3 Acceptance

Status: Completed.

Goal: make v0.3 reproducible and documented.

Planned:

- `docs/plan/v0.3-acceptance.md`.
- README updates for streaming, CLI output modes, store queries, LAN mDNS discovery, and mount PoC.
- Docker or local validation covers v0.3 additions.
- CI remains green.

Completed:

- Added `docs/plan/v0.3-acceptance.md`.
- Updated README for v0.3 commands and limits.
- Added `scripts/verify-v0.3-docker.sh`.
- Updated CI to run v0.3 Docker validation.
- Verified `scripts/verify-v0.3-docker.sh` locally against the two-node Docker environment.
- Updated this phase tracker after completing v0.3 implementation.

Remaining:

- Final CI status depends on the pushed branch run.

Done when:

- v0.3 has a canonical validation path.
- docs accurately describe runtime limits and commands.

## v0.4 Goal

Operon v0.4 should stabilize the runtime API, add a focused service/port
capability, and make trace/audit inspection more useful without expanding into
Web Console, clipboard, or screen/input work.

```text
v0.4 = stable runtime API + service/port capability + trace/audit UX.
```

v0.4 still does not implement port forwarding, proxying, VPN behavior, remote
desktop, clipboard, or Web Console.

## Phase 21: Runtime API Stabilization

Status: Completed.

Goal: make the daemon API predictable before adding more capability types.

Planned:

- shared API envelope types for success and error responses.
- stable error code naming.
- request/response schema documentation.
- SDK types aligned with daemon schemas.
- API-level tests for auth, policy denial, not found, and validation errors.
- notes on which interfaces may move to gRPC later.

Done when:

- API errors are consistent across core handlers.
- SDK and CLI parse the same structured error shape.
- docs describe current HTTP API boundaries and future gRPC candidates.

Completed:

- Added structured daemon errors with `code`, `message`, `status`, optional
  `capability`, and optional `resource`.
- Kept error parsing compatible for existing clients that only consume
  `code` and `message`.
- Aligned Rust core types and TypeScript SDK types with the daemon schema.
- Documented current HTTP API boundaries, structured errors, and future gRPC
  candidates in `docs/architecture/runtime-api.md`.
- Verified with Rust unit tests, SDK tests, and v0.4 Docker validation.

## Phase 22: Service / Port Capability

Status: Completed.

Goal: let nodes describe and health-check local services without becoming a proxy.

Planned:

- policy service allowlist.
- daemon service capability metadata.
- `/service/list` endpoint.
- `/service/check` endpoint.
- CLI `service list`.
- CLI `service check`.
- SDK service helpers.

Done when:

- configured services can be listed.
- allowed services can be health checked.
- denied services fail through policy.
- docs explicitly say this does not forward ports or proxy traffic.

Completed:

- Added service allowlist policy under `service.services`.
- Added daemon `/service/list` and `/service/check` endpoints.
- Added service capability metadata and audit events for allowed and denied
  service checks.
- Added CLI `service list` and `service check`.
- Added SDK `listServices` and `checkService`.
- Updated Docker policy fixtures and validation for listed, reachable, and
  denied services.
- Documented that service capability is metadata and TCP health checking only,
  not forwarding, proxying, relay, VPN, or reachability creation.

## Phase 23: Trace and Audit UX

Status: Completed.

Goal: make runtime observability useful from the CLI.

Planned:

- audit filters for capability, action, allowed, and resource/job id.
- trace list should target trace-like JSON files instead of every JSON file.
- trace show should have a human-readable summary mode and JSON mode.
- store query behavior documented.

Done when:

- audit output can be narrowed without shell filtering.
- trace list avoids unrelated JSON files.
- trace show is useful for humans and still scriptable.

Completed:

- Added CLI audit filters for `--capability`, `--action`, `--allowed`,
  `--resource`, and `--limit`.
- Updated `trace list` to only include trace-like JSON files.
- Updated `trace show` to default to a human-readable summary while preserving
  `--json` output.
- Covered audit filter and trace UX paths in `scripts/verify-v0.4-docker.sh`.

## Phase 24: v0.4 Acceptance

Status: Completed.

Goal: make v0.4 reproducible and documented.

Planned:

- `docs/plan/v0.4-acceptance.md`.
- README updates for API stability, service capability, and trace/audit UX.
- Docker validation covers service list/check, service policy denial, audit filters, and trace UX.
- CI runs v0.4 validation on every branch.

Done when:

- v0.4 has a canonical validation path.
- docs accurately describe runtime limits and commands.

Completed:

- Added `docs/plan/v0.4-acceptance.md`.
- Updated README with v0.4 validation, service commands, policy config, audit
  filters, and trace JSON/human usage.
- Added `scripts/verify-v0.4-docker.sh` and made it repeatable around the
  read-only mount PoC temp directory.
- Updated CI to run on pull requests and pushes to every branch.
- Updated CI Docker validation from v0.3 to v0.4.
- Verified `scripts/verify-v0.4-docker.sh` locally against the two-node Docker
  environment.

### v0.5: Web Console and Advanced Capabilities

- Web Console
- clipboard capability
- screen/input feasibility spike
- richer policy language
- trace visualization UI

## Planning Principle

Every phase should preserve the core boundary:

```text
Cloudflare Mesh / Tailscale / WireGuard / SSH / LAN solve connectivity.
Operon solves what connected machines are allowed to do, how execution is
composed, and how results are traced.
```
