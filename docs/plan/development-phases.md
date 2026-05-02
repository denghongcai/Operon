# Development Phases

This plan translates the current product and architecture decisions into an
implementation sequence.

This document is also the historical phase log. Earlier completed phases may
describe the runtime surface that existed at that time, including temporary
HTTP endpoints, daemon flags, and split config files. For the current runtime
contract, use `PROTOCOL.md`, `README.md`, `docs/architecture/runtime-api.md`,
and the latest completed phase entries.

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
- graphical management UI
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

- `operond start --grpc-listen <addr> --node-id <id>` implemented.
- daemon exposes `GET /health`.
- daemon exposes `GET /node`.
- manual YAML endpoint config is loadable.
- `operon node list` implemented.
- `operon node ping <node-id>` implemented for Phase 1 configured endpoints.
- `examples/nodes.yaml` includes a local endpoint.
- local validation passed with `operond` on `127.0.0.1:7789` and `operon node ping local`.
- Docker two-node validation added through `docker-compose.yml`, `docker/Dockerfile`, `examples/docker-nodes.yaml`, and `scripts/verify-mvp-docker.sh`.
- Docker two-node validation passed with `node-a` and `node-b`.

Remaining:

- None for Phase 1.

Commands:

```bash
operond start --grpc-listen 0.0.0.0:7789
operon node list
operon node ping cloud-a
```

Configuration:

```yaml
nodes:
  local:
    endpoint: grpc://127.0.0.1:7789
  cloud-a:
    endpoint: grpc://100.96.12.34:7789
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
  preserve_env: false
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
  { nodeId: "cloud-a", endpoint: "grpc://100.96.12.34:7789" },
  { nodeId: "gpu-node", endpoint: "grpc://100.96.18.20:7789" }
]);

const trace = await operon.run({
  name: "train-model",
  steps: [
    { node: "cloud-a", action: "fs.read", path: "/workspace/a.txt" },
    { node: "gpu-node", action: "job.run", command: "python train.py" }
  ]
});
```

Superseded by v0.5 and v0.5.1: SDKs should use the shared gRPC protocol, while
scripts and humans should use `operon --json`.

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
- Draft GitHub Release automation added for `v*` tags, producing Linux
  `x86_64`, `arm64`, and `armv7` binary tarballs, a JavaScript SDK tarball, and
  `SHA256SUMS` while leaving the release unpublished for manual review.

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

## Phase 11: Endpoint Resolver Cleanup

Status: Completed.

Goal: resolve configured endpoints without implementing connectivity.

Planned:

- manual resolver implementation.
- CLI `node resolve <node-id>`.
- validation that external networks are represented as ordinary endpoints.

Completed:

- Added endpoint resolution through `NodesConfig::resolve`.
- Added CLI `node resolve <node-id>`.
- Extended Docker validation to cover manual endpoint resolution.
- Later v0.8.16 removed the provider abstraction from the endpoint model.

Remaining:

- API discovery for external control planes is outside the runtime model.

Done when:

- config endpoints resolve to `grpc://` or `grpcs://` addresses.
- unsupported endpoint values fail clearly.
- Docker validation covers manual endpoint resolution.

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

- daemon can advertise node id, endpoint, and capability summary through LAN mDNS.
- CLI `node discover --timeout-secs 3`.
- discovered records are displayed and can optionally be written into a node config file.

Completed:

- Added daemon `--advertise-lan` mDNS advertisement.
- Added CLI `node discover --timeout-secs 3`.
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
- Added `scripts/verify-v0.5-docker.sh`.
- Updated CI to run v0.3 Docker validation.
- Verified `scripts/verify-v0.5-docker.sh` locally against the two-node Docker environment.
- Updated this phase tracker after completing v0.3 implementation.

Remaining:

- Final CI status depends on the pushed branch run.

Done when:

- v0.3 has a canonical validation path.
- docs accurately describe runtime limits and commands.

## v0.4 Goal

Operon v0.4 should stabilize the runtime API, add a focused service/port
capability, and make trace/audit inspection more useful without expanding into
CLI TUI console, clipboard, or screen/input work.

```text
v0.4 = stable runtime API + service/port capability + trace/audit UX.
```

v0.4 still does not implement port forwarding, proxying, VPN behavior, remote
desktop, clipboard, or CLI TUI console.

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
- Covered audit filter and trace UX paths in `scripts/verify-v0.5-docker.sh`.

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
- Added `scripts/verify-v0.5-docker.sh` and made it repeatable around the
  read-only mount PoC temp directory.
- Updated CI to run on pull requests and pushes to every branch.
- Updated CI Docker validation from v0.3 to v0.4.
- Verified `scripts/verify-v0.5-docker.sh` locally against the two-node Docker
  environment.

## v0.5 Goal

Operon v0.5 should replace the temporary HTTP-only runtime path with a real
gRPC core protocol. v0.5 temporarily kept the HTTP/JSON facade during migration;
v0.5.1 should remove it and make CLI/SDK the supported interfaces.

```text
v0.5 = protobuf source of truth + tonic gRPC server + gRPC-capable CLI paths.
```

v0.5 does not introduce a new network layer, HTTPS/mTLS, graphical UI, port
forwarding, or mount behavior changes.

## Phase 25: Protocol Contract Finalization

Status: Completed.

Goal: make protobuf the authoritative runtime contract before wiring the server.

Planned:

- align `proto/operon/*.proto` with the current HTTP API surface.
- cover node metadata, health, capability list, fs, jobs, audit, scoped secret
  use through jobs, and service checks.
- model streaming file reads, streaming file writes, followed job logs, stdin,
  and execution events explicitly.
- define auth metadata conventions for bearer tokens.
- add generated Rust types through `operon-protocol`.
- document compatibility between migration-era HTTP facade errors and gRPC
  status details.

Done when:

- protobuf schemas cover the current v0.4 runtime capabilities.
- `operon-protocol` builds generated Rust bindings.
- protocol docs describe which methods are unary, server-streaming,
  client-streaming, or bidirectional.

Completed:

- Added `proto/operon/runtime.proto` as the v0.5 runtime contract.
- Generated Rust bindings from `operon-protocol` with tonic/prost.
- Modeled bearer auth metadata, unary runtime methods, server-streaming file
  reads/job logs, and client-streaming file writes/job stdin.

## Phase 26: gRPC Daemon Server

Status: Completed.

Goal: expose the current daemon capability runtime through tonic.

Planned:

- add a gRPC listener to `operond`.
- keep HTTP/JSON enabled as a v0.5-only migration facade.
- route gRPC calls through the same policy, auth, audit, store, and execution
  code paths.
- support streaming fs read/write and job log/stdin paths through gRPC.
- preserve structured audit and trace output.

Done when:

- Docker can start two daemons with gRPC endpoints.
- gRPC calls enforce the same authorization behavior as the existing runtime.
- streaming gRPC paths are covered by tests or Docker validation.

Completed:

- Added optional `operond start --grpc-listen` during the migration window.
- Routed gRPC calls through the same fs, job, service, secret, policy, and audit
  state used by the existing runtime.
- Added two-node Docker gRPC exposure on `7789`.

## Phase 27: gRPC CLI and SDK Bridge

Status: Completed.

Goal: let the CLI use gRPC for runtime operations during the migration window.

Planned:

- add CLI transport selection, defaulting to gRPC for supported operations.
- preserve temporary fallback for debugging and transitional compatibility.
- expose clear endpoint config for `grpc://` node URLs.
- update the TypeScript SDK contract to match protobuf schemas.

Done when:

- core CLI commands can run against gRPC endpoints.
- existing examples still work.
- docs clearly explain the migration from HTTP to gRPC.

Completed:

- Added Rust CLI support for `grpc://` and `grpcs://` endpoints during the
  migration window.
- Updated graph execution to use gRPC when node endpoints are gRPC endpoints.
- Updated the TypeScript SDK to generate protobuf bindings and use
  `nice-grpc` for gRPC endpoints.

## Phase 28: v0.5 Acceptance

Status: Completed.

Goal: make the gRPC migration reproducible.

Planned:

- `docs/plan/v0.5-acceptance.md`.
- Docker validation for two gRPC-connected nodes.
- CI updates to run the v0.5 validation path.
- README updates for gRPC endpoint config and protocol status.

Done when:

- v0.5 has one canonical validation command.
- HTTP facade and gRPC runtime behavior are both covered.
- `docs/plan/development-phases.md` records completed v0.5 work.

Completed:

- Added `scripts/verify-v0.5-docker.sh` as the canonical two-node gRPC
  validation.
- Added `examples/docker-nodes.yaml`.
- Updated CI with v0.5 Docker validation and `protoc` installation for Rust
  protocol generation.
- Verified locally with `cargo test --workspace`, `pnpm typecheck`,
  `pnpm test`, and `scripts/verify-v0.5-docker.sh`.

## v0.5.1 Cleanup Goal

Operon v0.5.1 should remove the HTTP runtime facade now that the canonical
runtime protocol is gRPC.

```text
v0.5.1 = gRPC daemon runtime + CLI/SDK interfaces, no parallel HTTP runtime API.
```

The reason is product and maintenance clarity: Operon already has `operon CLI`
for humans, ops, and scripts, including `--json` for machine-readable output.
Keeping direct HTTP runtime access creates a second public API surface, doubles
auth/error/streaming semantics, expands daemon exposure, and makes users choose
between CLI, SDK, HTTP, and gRPC.

v0.5.1 does not introduce HTTPS/mTLS, signed node identity, Linux mount support,
TUI console work, agent integration, or non-LAN discovery.

## Phase 28.1: HTTP Runtime Facade Removal

Status: Completed.

Goal: make gRPC the only daemon runtime API.

Planned:

- remove axum runtime routes and HTTP handler code from `operond`.
- remove the hand-written HTTP client path from `operon-cli`.
- remove `http://` node endpoint support from runtime configs and examples.
- keep CLI `--json` as the supported script interface.
- keep TypeScript SDK on `nice-grpc`.
- update Docker and CI validation to use gRPC endpoints only.
- update runtime architecture docs to describe HTTP as removed, not retained as
  a facade.
- add `PROTOCOL.md` for direct generated-client integration without an SDK.

Done when:

- `operond` exposes runtime operations through gRPC only.
- core CLI commands still work against `grpc://` endpoints.
- `operon --json` covers scriptable output for node, fs, job, service, audit,
  and graph workflows.
- no docs describe direct HTTP runtime calls as a supported product surface.
- `docs/plan/v0.5.1-cleanup-acceptance.md` records the validation.

Completed:

- `operond` now exposes runtime operations through gRPC only.
- `operon-cli` runtime commands use the gRPC client path only.
- TypeScript SDK calls `nice-grpc` directly and no longer has fetch/HTTP
  fallback behavior.
- Docker, CI, and example node configs use `grpc://` endpoints.
- Root `PROTOCOL.md` documents direct protocol integration without an SDK.
- Validation passed with `cargo fmt --check`, `cargo check --workspace
  --locked`, `cargo test --workspace --locked`, `cargo clippy --workspace
  --locked -- -D warnings`, `pnpm typecheck`, `pnpm -r test`,
  `pnpm --filter @operon/sdk build`, `scripts/verify-v0.5-docker.sh`, and
  `git diff --check`.

Remaining:

- HTTPS/mTLS and signed node identity remain later hardening work.
- Linux real mount work starts in v0.6.

## v0.6 Goal

Operon v0.6 should turn the read-only mount proof of concept into a real Linux
read-only mount adapter.

```text
v0.6 = Linux read-only FUSE mount over the Operon fs protocol.
```

v0.6 only targets Linux and read-only behavior. WinFsp, macOS mount support,
write support, offline sync, distributed cache invalidation, and multi-writer
conflict resolution remain out of scope.

## Phase 29: Linux Mount Contract

Status: Completed.

Goal: define the mount semantics before implementing FUSE behavior.

Planned:

- decide read-only versus read-write scope for the first Linux mount.
- define path normalization, mount-root mapping, cache behavior, and error
  mapping.
- document consistency limits and operations that are intentionally unsupported.
- keep FUSE as an adapter over the fs protocol, not as a separate capability
  model.

Completed:

- Decided v0.6 is read-only live FUSE mount.
- Documented absolute remote path normalization, `..` rejection, child-name
  validation, one-second metadata TTL, and read-only permission exposure in
  `docs/plan/v0.6-acceptance.md`.
- Kept write mount behavior out of v0.6 and added v0.6.1 as the dedicated write
  mount phase.

Done when:

- mount behavior is documented in `docs/plan/v0.6-acceptance.md`.
- policy and audit requirements are explicit.
- unsupported semantics are listed before implementation.

## Phase 30: Linux FUSE Adapter

Status: Completed.

Goal: implement a real Linux mount path in `operon-mount`.

Planned:

- add Linux-only FUSE dependencies.
- implement lookup, getattr, readdir, open, read, and release.
- keep write operations out of v0.6.
- route all remote fs operations through existing policy-enforced daemon APIs.
- record audit events through the remote node for mounted operations.

Completed:

- Added `fuser` and implemented a read-only FUSE adapter in `operon-mount`.
- Added a `RemoteFs` trait as the Core FS Protocol boundary and kept the Linux
  FUSE code as an OS mount adapter over that trait.
- Kept direct gRPC access through `GrpcRemoteFs`; local IPC is not part of v0.6.
- Implemented inode mapping, lookup, getattr, readdir, read-only open, ranged
  reads over the gRPC `ReadFile` stream, and release.
- Routed stat/list/read through the existing gRPC runtime API so policy and audit
  remain daemon-owned.
- Moved the workspace Rust baseline to 1.85 to use the current pure-Rust fuser
  release.

Done when:

- `operon mount` creates a live Linux mount.
- reads through the mount reflect remote node content.
- path escapes remain impossible.
- unmount cleanup is reliable.

## Phase 31: Mount CLI UX and Validation

Status: Completed.

Goal: make Linux mount usable and testable from the CLI.

Planned:

- add `operon mount <node:/path> --to <dir>` for Linux.
- add foreground mode and clean signal handling.
- add a Docker or Linux-only validation path for mount behavior.
- document host requirements such as `/dev/fuse` and privileges.

Completed:

- Replaced the v0.3 `mount read-only` one-shot materialization CLI with
  `operon mount <node:/path> --to <dir>`.
- The command starts a foreground FUSE session and unmounts on Ctrl-C.
- Added `scripts/verify-v0.6-linux-mount.sh` with host requirement checks.
- Updated README with the live mount command and read-only limitations.

Done when:

- mount validation runs on Linux developer machines.
- CI either validates FUSE where available or clearly gates the test.
- README includes the Linux mount command and limitations.

## Phase 32: v0.6 Acceptance

Status: Completed.

Goal: make the Linux mount milestone reproducible.

Planned:

- `docs/plan/v0.6-acceptance.md`.
- Linux mount validation script.
- README and AGENTS updates for mount scope.

Completed:

- Updated `docs/plan/v0.6-acceptance.md` with the read-only live mount
  contract.
- Added v0.6.1 write mount planning before v0.7.
- Updated README and AGENTS for the current mount milestone.
- Validated with `cargo fmt --check`, `cargo check --workspace --locked`,
  `cargo test --workspace --locked`, `cargo clippy --workspace --locked -- -D warnings`,
  `pnpm typecheck`, `pnpm -r test`, `scripts/verify-v0.5-docker.sh`,
  `scripts/verify-v0.6-linux-mount.sh`, and `git diff --check`.

Done when:

- real Linux mount behavior replaces the mount PoC in the roadmap.
- validation covers mount, read, policy denial, audit, and cleanup.

## v0.6.1 Goal

Operon v0.6.1 should add Linux write mount support after the read-only FUSE path
is stable.

```text
v0.6.1 = Linux write-through FUSE mount over the Operon fs protocol.
```

v0.6.1 remains Linux-only and should use single-writer, write-through semantics.
It should not introduce offline sync, distributed cache invalidation,
multi-writer conflict resolution, append atomicity guarantees beyond the remote
filesystem, or cross-platform mount adapters.

## Phase 32.1: Linux Write Mount Contract

Status: Completed.

Goal: define write semantics before adding write-capable FUSE operations.

Planned:

- define create, write, flush, fsync, truncate, unlink, mkdir, rmdir, and rename
  semantics.
- define close-to-open versus write-through behavior.
- define how write failures map to FUSE errors.
- keep daemon policy and audit authoritative for all write operations.

Done when:

- write mount behavior is documented.
- consistency and unsupported semantics are explicit.
- validation expectations for write, delete, rename, and denied writes are
  recorded.

Completed:

- Documented Linux-only, single-writer, write-through semantics in
  `docs/plan/v0.6.1-acceptance.md`.
- Documented that v0.6.1 has no file versions, etags, locks, leases, CAS
  preconditions, snapshot reads, or multi-writer conflict detection.
- Defined FUSE operation mapping to Core FS Protocol RPCs:
  `WriteFileRange`, `TruncateFs`, `MkdirFs`, `DeleteFs`, and `RenameFs`.
- Kept policy, audit, and path containment in `operond`.

## Phase 32.2: Linux Write FUSE Adapter

Status: Completed.

Goal: add write-capable FUSE operations to the Linux mount adapter.

Planned:

- implement create, write, flush, fsync, setattr/truncate, unlink, mkdir, rmdir,
  and rename where supported by the daemon fs protocol.
- add or extend daemon fs protocol operations if the current write-file API is
  insufficient.
- route write operations through existing policy and audit paths.

Done when:

- write-through file creation and updates work through the mount.
- denied write/delete/rename operations are audited.
- cleanup remains reliable after write failures.

Completed:

- Extended `proto/operon/runtime.proto` with write-range, truncate, mkdir,
  delete, and rename fs RPCs.
- Implemented the new daemon gRPC handlers through the existing fs policy and
  audit path.
- Extended `RemoteFs` and `GrpcRemoteFs` with write-capable operations.
- Updated the Linux FUSE adapter to support create, write, flush, fsync,
  truncate, unlink, mkdir, rmdir, and rename.
- Left symlink, hardlink, special node creation, offline sync, and cross-platform
  adapters out of scope.

## Phase 32.3: v0.6.1 Acceptance

Status: Completed.

Goal: make write mount behavior reproducible and separately releasable.

Planned:

- v0.6.1 acceptance document.
- Linux write mount validation script.
- README and AGENTS updates for write mount scope.

Done when:

- v0.6.1 has documented acceptance criteria.
- validation covers create, write, read-after-write, truncate, delete, rename,
  denied writes, audit, and cleanup.

Completed:

- Updated `docs/plan/v0.6.1-acceptance.md` with final contract and completion
  notes.
- Added `scripts/verify-v0.6.1-linux-write-mount.sh`.
- Updated CI with a v0.6.1 Linux write mount validation job.
- Updated README, AGENTS, and architecture docs for the write-through mount
  milestone.

Remaining:

- None for v0.6.1.

## v0.6.2 Cleanup Goal

Operon v0.6.2 should align the CLI with the filesystem mutation operations that
were added for the v0.6.1 write-through mount.

```text
v0.6.2 = CLI fs mutation commands over the existing Core FS Protocol.
```

v0.6.2 is a cleanup phase. It should not add new daemon capabilities, mount
modes, endpoint discovery UX, or a new runtime API surface. `WriteFileRange` remains
available through the protocol for mount adapters and direct clients, but normal
CLI users should use higher-level commands.

v0.6.2 inherits the v0.6.1 filesystem concurrency contract: CLI mutation
commands do not perform conflict detection, version checks, locks, or leases.

## Phase 32.4: CLI FS Mutation Surface

Status: Completed.

Goal: expose the daemon fs mutation RPCs through user-facing CLI commands.

Planned:

- add `operon fs mkdir <node:/path>`.
- add `operon fs rm <node:/path>`.
- add `operon fs rename <node:/from> <node:/to>`.
- add `operon fs truncate <node:/path> --size <bytes>`.
- keep `WriteFileRange` out of the normal CLI surface unless a concrete
  operator use case appears.

Done when:

- CLI command coverage matches the user-facing v0.6.1 fs mutation model.
- commands use the existing gRPC runtime API.
- daemon policy and audit remain authoritative.

Completed:

- Added CLI gRPC helpers for `MkdirFs`, `DeleteFs`, `RenameFs`, and
  `TruncateFs`.
- Added `operon fs mkdir <node:/path>`.
- Added `operon fs rm <node:/path>`.
- Added `operon fs rename <node:/from> <node:/to>`.
- Added `operon fs truncate <node:/path> --size <bytes>`.
- Kept `WriteFileRange` out of the normal CLI surface.

## Phase 32.5: v0.6.2 Acceptance

Status: Completed.

Goal: make the CLI cleanup reproducible before starting v0.7.

Planned:

- `docs/plan/v0.6.2-cli-fs-cleanup-acceptance.md`.
- validation script for CLI fs mutation commands.
- README, PROTOCOL, and AGENTS updates.

Done when:

- validation covers mkdir, truncate, rename, rm, denied mutations, and audit.
- docs clearly state that `WriteFileRange` remains a low-level protocol
  operation.
- v0.7 can start without a known CLI/runtime fs mismatch.

Completed:

- Added `docs/plan/v0.6.2-cli-fs-cleanup-acceptance.md`.
- Added `scripts/verify-v0.6.2-cli-fs-cleanup.sh`.
- Updated README, PROTOCOL, AGENTS, and CI for v0.6.2.

Remaining:

- None for v0.6.2.

## v0.6.3 Goal

Operon v0.6.3 should add same-node filesystem copy as a protocol, CLI, and SDK
convenience operation.

```text
v0.6.3 = daemon-side same-node fs copy.
```

v0.6.3 does not add cross-node copy, recursive directory copy, sparse-file
preservation, metadata preservation, copy-on-write clone semantics, or conflict
detection. Mount/FUSE still observes POSIX read/write/create operations for
tools such as `cp`.

## Phase 32.6: FS Copy Protocol And Interfaces

Status: Completed.

Goal: expose a same-node copy operation without routing file bytes through the
CLI or SDK process.

Planned:

- add `CopyFs(from_path, to_path)` to the runtime protocol.
- implement daemon-side copy within one node workspace.
- require read permission on the source and write permission on the target.
- add `operon fs copy <node:/from> <node:/to>`.
- add TypeScript SDK `copyFile` and `fs.copy` support.
- keep cross-node copy out of scope.

Done when:

- protocol, CLI, and SDK expose same-node copy.
- daemon audit records allowed and denied copy operations.
- copy remains scoped to regular files.

Completed:

- Added `CopyFs`, `FsCopyRequest`, and `FsCopy` to
  `proto/operon/runtime.proto`.
- Implemented daemon-side same-node regular-file copy with workspace
  containment, read/write policy checks, and audit action `copy`.
- Added `operon fs copy <node:/from> <node:/to>`.
- Added `OperonClient.copyFile` and SDK `fs.copy` graph action support.

## Phase 32.7: v0.6.3 Acceptance

Status: Completed.

Goal: make same-node copy behavior reproducible before starting v0.7.

Planned:

- `docs/plan/v0.6.3-fs-copy-acceptance.md`.
- validation script for allowed and denied fs copy.
- README, PROTOCOL, AGENTS, and CI updates.

Done when:

- validation covers allowed copy, denied source read, denied target write, and
  audit records.
- docs clearly state that cross-node copy remains later work.

Completed:

- Added `docs/plan/v0.6.3-fs-copy-acceptance.md`.
- Added `scripts/verify-v0.6.3-fs-copy.sh`.
- Updated README, PROTOCOL, AGENTS, and CI for v0.6.3.

Remaining:

- None for v0.6.3.

## v0.6.4 Cleanup Goal

Operon v0.6.4 should make first-run configuration faster without replacing the
scriptable CLI setup commands.

```text
v0.6.4 = onboard as a guided wrapper over existing setup primitives.
```

`operon onboard` must generate normal Operon files and show the equivalent CLI
commands. `init config`, `node discover --timeout-secs 3`, and other
command-style configuration paths remain the stable automation surface.

## Phase 32.8: Onboard Command

Status: Completed.

Goal: add a guided setup entrypoint for daemon/client first-run configuration.

Planned:

- add `operon onboard`.
- support daemon, client, and combined setup roles.
- generate `config.yaml`, `token`, and daemon start command
  files under a chosen output directory.
- support optional LAN discovery as an onboarding input.
- support coarse capability preauthorization for generated policy files.
- print equivalent command-style CLI operations.
- do not add onboard-only config formats or remote policy mutation.

Done when:

- onboard output can be inspected and edited as ordinary Operon config.
- command-style setup remains available for scripts and CI.
- generated daemon/client config can run a local validation flow.

Completed:

- Added `operon onboard` with daemon, client, and combined roles.
- Added non-interactive `--yes` mode for reproducible setup generation.
- Generated ordinary `config.yaml`, `token`, and `daemon-command.txt` files.
- Added optional LAN discovery input for client node config generation.
- Added coarse capability grant selection for generated policy files.
- Printed equivalent CLI setup commands after onboarding.

## Phase 32.9: v0.6.4 Acceptance

Status: Completed.

Goal: make onboarding reproducible before starting v0.7.

Planned:

- `docs/plan/v0.6.4-onboard-acceptance.md`.
- validation script for generated config, daemon startup, CLI ping,
  capability inspection, fs operation, and audit inspection.
- README, AGENTS, and CI updates.

Done when:

- validation covers generated config files and a live daemon flow.
- docs state that onboard is a convenience wrapper, not a second config system.

Completed:

- Added `docs/plan/v0.6.4-onboard-acceptance.md`.
- Added `scripts/verify-v0.6.4-onboard.sh`.
- Updated README, AGENTS, and CI for v0.6.4.

Remaining:

- None for v0.6.4.

## v0.6.5 Cleanup Goal

Operon v0.6.5 should replace split configuration files with one unified
configuration entrypoint.

```text
v0.6.5 = one config.yaml schema for daemon, client, policy, auth, store, and secret references.
```

Daemon and CLI-specific settings remain separate sections inside the same file.
Sensitive values should be referenced through `token_file`, `token_env`, or
`secrets.file` instead of being forced inline.

## Phase 32.10: operon-config Crate

Status: Completed.

Goal: put configuration ownership in a dedicated crate instead of `operon-network`.

Planned:

- add `operon-config`.
- define `OperonConfig`, daemon config, client config, node config, auth config,
  and secret references.
- keep provider values available for client node resolution.
- resolve relative file references from the config file directory.

Completed:

- Added `operon-config` as the shared schema/loading crate.
- Moved unified config, node endpoint, provider, auth, daemon, client, and
  secret reference types into `operon-config`.
- Kept `operon-network` as a thin re-export boundary for provider/node endpoint
  types.

## Phase 32.11: Unified CLI And Daemon Config

Status: Completed.

Goal: make `config.yaml` the supported runtime configuration entrypoint.

Planned:

- make `operon --config` read unified `client.nodes`.
- make `operond start --config` read daemon, policy, auth, store, and secrets.
- default missing `--config` to `$HOME/.operon/config.yaml`.
- remove legacy split config assumptions from onboard and validation scripts.

Completed:

- CLI now loads node endpoints from unified config.
- Daemon now starts from unified config.
- CLI and daemon default to `$HOME/.operon/config.yaml` when `--config` is not
  provided.
- Onboard now writes `.operon/config.yaml`, `.operon/token`, and
  `.operon/daemon-command.txt`.
- Docker and validation configs were moved to unified config files.

## Phase 32.12: v0.6.5 Acceptance

Status: Completed.

Goal: validate unified config before starting v0.7.

Planned:

- `docs/plan/v0.6.5-unified-config-acceptance.md`.
- validation for onboard-generated unified config.
- validation for daemon/client fs mutation and copy flows through unified config.
- README, AGENTS, and CI updates.

Completed:

- Added `docs/plan/v0.6.5-unified-config-acceptance.md`.
- Updated onboard, Docker config, and validation scripts for unified config.
- Updated README, AGENTS, and CI references for v0.6.5.
- Split README user Quickstart from developer validation, with release-download
  install commands and onboard-based setup.
- Optimized GitHub Actions with Rust build caching, a matrix-backed validation
  job, and parallel release matrix builds for Linux `x86_64`, `arm64`, and
  `armv7` archives.

Remaining:

- None for v0.6.5.

## Phase 32.13: Runtime Correctness Cleanup

Status: Completed.

Goal: address post-v0.6.5 review findings before v0.7 feature work.

Planned:

- drain job stdout/stderr capture tasks before marking jobs terminal.
- bound the in-memory audit log while keeping store append behavior.
- extract LAN mDNS discovery into the network crate.
- reuse the CLI Tokio runtime across synchronous gRPC helper calls.

Completed:

- Job stdout/stderr capture tasks are awaited before jobs are marked terminal,
  so terminal job records and live log streams include drained output.
- In-memory audit retention is capped while store append behavior remains
  unchanged.
- LAN mDNS discovery is centralized in `operon-network` and reused by `node
  discover` and `onboard`.
- CLI gRPC helpers reuse one process-local Tokio runtime instead of creating a
  new runtime for every call.

Remaining:

- None for Phase 32.13.

## Phase 32.14: Job Event Stream And Log Storage Split

Status: Completed.

Goal: replace polling-based job waits and embedded job logs with streaming job
events plus separate bounded log storage.

Planned:

- add a gRPC `WatchJob` stream for job status changes.
- keep job logs behind dedicated log APIs instead of embedding them in
  `JobRecord`.
- store job logs in an append-only store record path and a bounded in-memory
  ring buffer.
- update CLI graph/wait/log paths and the TypeScript SDK to use streaming job
  status rather than fixed polling.

Completed:

- Added `WatchJob` and `ListJobLogs` to the gRPC protocol.
- Removed embedded logs from `JobRecord`; job records now report `log_count`
  and `logs_truncated`.
- Moved daemon job logs into append-only store records plus a bounded in-memory
  ring buffer.
- Updated CLI job wait, graph execution, and TypeScript SDK job execution to
  wait through `WatchJob` instead of fixed polling.
- Updated CLI/SDK log reads to use dedicated job log APIs.

Remaining:

- None for Phase 32.14.

## v0.6.6 Release Goal

Operon v0.6.6 should stabilize and release the job event/log protocol cleanup
before the next feature milestone starts.

```text
v0.6.6 = WatchJob status stream, separate job log APIs, and bounded job log retention.
```

v0.6.6 should not add new runtime capabilities. It is an acceptance and release
checkpoint for the protocol-breaking job record cleanup.

## Phase 32.15: v0.6.6 Release Hardening

Status: Completed.

Goal: close the release-blocking security and streaming gaps before tagging
v0.6.6.

Completed:

- enforced real workspace containment for symlink-resolving filesystem and job
  cwd operations.
- applied `job.env_allowlist` by clearing inherited daemon environment and
  injecting only allowed variables plus authorized secrets.
- propagated execution graph run/step context into audit events through gRPC
  metadata.
- streamed CLI file reads and job logs directly to writers instead of buffering
  whole streams in memory.
- exposed a true TypeScript SDK file stream API while keeping read-all helpers.
- added `docs/plan/v0.6.6-acceptance.md` and release validation notes.

Remaining:

- None for Phase 32.15.

## Phase 32.16: Runtime Crate Boundary Refactor

Status: Completed.

Goal: create stable crate APIs for runtime areas that later service, agent, and
provider milestones will reuse.

Completed:

- moved workspace path containment and fs policy helpers into `operon-fs`.
- moved job authorization and environment construction into `operon-process`.
- moved append-only store helpers into `operon-store`.
- moved service health check helper into `operon-network`.
- updated daemon code to use these crate APIs instead of local copies.

Remaining:

- Further daemon decomposition can happen before v0.7 if needed, but no
  release-blocking runtime helper duplication remains in this phase.

## Phase 32.17: Job Environment Preserve Option

Status: Completed.

Goal: allow deployments to opt into preserving the daemon environment for jobs
without making full environment inheritance the default.

Completed:

- added `policy.job.preserve_env` with a default of `false`.
- kept job spawning on `env_clear()` so the daemon always passes an explicit
  environment map.
- when `preserve_env: true`, job environment construction includes all daemon
  environment variables before applying authorized secrets.
- documented the security tradeoff in README and PROTOCOL.

Remaining:

- None for Phase 32.17.

## Phase 32.18: v0.6.6 Security Review Follow-up

Status: Completed.

Goal: close security and semantic issues found after the v0.6.6 hardening
review.

Completed:

- onboard token generation now uses OS CSPRNG and fails instead of falling back
  to predictable values.
- onboard token files are written with owner-only permissions on Unix, and
  existing private files with group/world permissions or symlink paths are
  rejected.
- `fs rm` and `fs rename` now use leaf-symlink semantics so they operate on the
  symlink entry instead of the canonical target.
- documented the remaining path-based containment TOCTOU limit and the longer
  term `openat2(RESOLVE_BENEATH)` direction.
- CLI job command assembly now preserves argument boundaries for multiple CLI
  tokens while preserving single-token shell command strings.

Remaining:

- Long term: replace path-based workspace traversal with fd-relative Linux
  `openat2` resolution.
- Long term: add protocol-level `argv[]` job execution if shell-free command
  execution becomes a product requirement.

## v0.6.7 Goal

Operon v0.6.7 should close the remaining runtime infrastructure issues before
the next feature milestone.

```text
v0.6.7 = process lifecycle, binary job logs, and explicit async CLI runtime.
```

This milestone should not add new user-facing capabilities. It should make the
existing job and CLI surfaces safer and cleaner so v0.7 can reuse them without
carrying avoidable runtime debt.

## Phase 32.19: Job Process Group Termination

Status: Completed.

Goal: make job cancellation and timeout terminate the whole Linux job process
tree, not only the direct shell child.

Planned:

- on Linux, start each job in its own process group or session.
- on cancel and timeout, send termination to the process group instead of only
  calling `start_kill()` on the direct child.
- keep the current `kill_on_drop(true)` behavior as a last-resort cleanup guard,
  not as the primary lifecycle mechanism.
- document Linux-only semantics and the non-Linux fallback if the code remains
  portable at compile time.
- add a test or validation script that starts a child which spawns a long-lived
  descendant, cancels the job, and verifies the descendant exits.

Completed:

- Linux job commands now start in their own process group.
- cancel and timeout now signal the process group with `SIGTERM`, escalate to
  `SIGKILL` after a short wait, and keep direct-child kill as the non-Unix
  fallback.
- added `scripts/verify-v0.6.7-runtime.sh` to validate descendant termination
  through the CLI and daemon.

Done when:

- cancelling a Linux job reliably terminates shell-spawned descendants.
- timing out a Linux job follows the same process-group termination path.
- job status and audit behavior remain unchanged for cancel and timeout.

## Phase 32.20: Binary-Safe Job Log Protocol

Status: Completed.

Goal: preserve stdout/stderr bytes end to end instead of forcing job logs
through UTF-8 strings.

Planned:

- change runtime `JobLog.data` from `string` to `bytes` in the gRPC protocol.
- update Rust core/protocol conversions, daemon capture, ring buffer storage,
  job log listing, and job log streaming to use byte buffers internally.
- update CLI output paths to write log bytes directly to stdout/stderr sinks,
  only decoding for JSON or human display when unavoidable.
- update the TypeScript SDK so `streamJobLogs` yields `Uint8Array` chunks
  without `TextEncoder` re-encoding.
- document the protocol change in `PROTOCOL.md`.

Completed:

- runtime `JobLog.data` is now `bytes` in the gRPC protocol.
- Rust core, protocol conversion, daemon capture, ring buffer storage, and CLI
  log output now preserve log bytes.
- TS generated bindings and SDK types now expose job log data as `Uint8Array`.
- SDK tests cover byte-preserving `streamJobLogs`.
- README and PROTOCOL document binary-safe log behavior.

Done when:

- non-UTF-8 stdout/stderr survives daemon capture, CLI streaming, and TS SDK
  streaming without data loss.
- list and stream log APIs have consistent binary-safe semantics.
- generated TS protocol bindings and SDK tests are updated.

## Phase 32.21: Explicit Async CLI Runtime

Status: Completed.

Goal: remove the hidden singleton Tokio runtime from the CLI gRPC layer and make
the CLI entrypoint own async execution explicitly.

Planned:

- convert `operon-cli` entrypoint to an explicit Tokio runtime, preferably
  `#[tokio::main] async fn main()`.
- convert `crates/operon-cli/src/grpc.rs` public gRPC helper functions to
  async functions.
- remove `OnceLock<tokio::runtime::Runtime>` and the internal `block_on`
  wrapper from `grpc.rs`.
- propagate `.await` through CLI command handlers and graph execution where
  they call gRPC.
- preserve synchronous local file/config parsing where there is no runtime
  benefit to changing it.
- keep `operon_mount::spawn_mount` unchanged unless the mount command requires a
  follow-up integration adjustment.

Completed:

- CLI entrypoint now owns the Tokio runtime explicitly.
- `operon-cli/src/grpc.rs` no longer owns a singleton runtime or internal
  `block_on` wrapper.
- gRPC helper functions are async and command handlers/graph execution await
  them directly.
- request context propagation moved to a Tokio task-local so graph audit
  metadata survives async execution.

Done when:

- `operon-cli/src/grpc.rs` no longer creates or owns a Tokio runtime.
- all gRPC calls are awaited from the CLI command path.
- graph audit context propagation still works after async conversion.
- CLI tests and Docker smoke validation pass.

## Phase 32.22: v0.6.7 Acceptance

Status: Completed.

Goal: make the infrastructure cleanup reproducible before moving to v0.7.

Planned:

- create `docs/plan/v0.6.7-acceptance.md`.
- add validation commands for process-group cancellation, binary log streaming,
  and async CLI runtime behavior.
- update README and PROTOCOL only where user-visible behavior or protocol shape
  changes.
- update AGENTS.md after completion so the next planned milestone is clear.

Completed:

- created and executed `scripts/verify-v0.6.7-runtime.sh`.
- updated `docs/plan/v0.6.7-acceptance.md`, README, PROTOCOL, and AGENTS.md.
- completed Rust, SDK, and runtime validations for v0.6.7.

Done when:

- `cargo fmt --check` passes.
- `cargo clippy --workspace --locked -- -D warnings` passes.
- `cargo test --workspace --locked` passes.
- `pnpm --filter @operon/sdk typecheck` passes.
- Docker-backed runtime validation covers the changed job and log behavior.

## v0.6.8 Goal

Operon v0.6.8 should stabilize the gRPC schema before building more operator
interfaces on top of it.

```text
v0.6.8 = schema-level protocol constraints and cleanup.
```

This milestone should remove avoidable ambiguity from `runtime.proto`. It should
prefer schema-enforced contracts over stringly typed fields and prose-only
rules, while preserving the product boundary that Operon exposes one runtime
control plane over gRPC.

## Phase 32.23: Typed Runtime Enums

Status: Completed.

Goal: replace wire-level string enum fields with protobuf enum types.

Planned:

- add protobuf enums for capability kind, job status, and service protocol.
- change `Capability.kind`, `JobRecord.status`, and `JobEvent.status` to use
  typed enum fields.
- update Rust core/protocol conversions and TypeScript generated bindings.
- preserve stable human-facing names in CLI/JSON output through explicit
  formatting rather than wire strings.

Done when:

- invalid capability kind and job status values cannot be represented by normal
  generated clients.
- Rust and TS tests cover enum conversion.
- PROTOCOL.md documents the typed enum values.

Completed:

- Added protobuf enums for capability kind, job status, and service protocol.
- Updated Rust and TypeScript conversions to map protobuf enums to stable
  human-facing CLI/JSON names.
- Documented enum values in PROTOCOL.md.

## Phase 32.24: Proto3 Optional Presence

Status: Completed.

Goal: replace manual `has_*` presence flags with protobuf presence semantics.

Planned:

- use proto3 `optional` for timeout, exit code, reason, run id, and step id
  fields where absence is meaningful.
- remove `has_timeout_secs`, `has_exit_code`, `has_reason`, `has_run_id`, and
  `has_step_id` from active message schemas.
- update Rust and TS conversions to use generated option fields.
- reserve removed field numbers where needed to prevent accidental reuse.

Done when:

- generated clients expose absence through optional fields instead of paired
  boolean flags.
- runtime behavior for omitted timeout, exit code, reason, run id, and step id
  is unchanged.

Completed:

- Replaced manual `has_*` fields with proto3 optional presence for timeout,
  exit code, reason, run id, and step id.
- Reserved removed field numbers and names in `runtime.proto`.
- Updated Rust and TypeScript client/server conversions to use optional fields.

## Phase 32.25: Proto Surface Pruning

Status: Completed.

Goal: remove or quarantine legacy proto files that are not part of the active
runtime surface.

Planned:

- decide whether `proto/operon/node.proto`, `execution.proto`, `policy.proto`,
  and `capability.proto` are archive-only or should be deleted.
- if kept, move them under an archive path or add explicit comments that they
  are non-compiled design leftovers.
- ensure build scripts and docs point only to the active runtime proto.

Done when:

- contributors cannot reasonably mistake legacy proto files for live services.
- `crates/operon-protocol/build.rs` remains focused on the active runtime API.

Completed:

- Moved inactive proto files to `proto/archive/operon/`.
- Kept `crates/operon-protocol/build.rs` focused on
  `proto/operon/runtime.proto`.
- Updated architecture docs to identify `runtime.proto` as the only active
  runtime protocol.

## Phase 32.26: Streaming Request Envelopes

Status: Completed.

Goal: make client-streaming file and stdin contracts explicit in the protobuf
schema rather than relying only on first-message conventions.

Planned:

- redesign `WriteFile` streaming messages so target metadata and data chunks are
  represented as distinct message variants.
- redesign `WriteJobStdin` streaming messages so job identity and data chunks
  are represented as distinct message variants.
- keep daemon-side validation for ordering and duplicate metadata.
- update CLI and SDK chunk producers to use the new envelope shape.
- document the streaming contract in PROTOCOL.md.

Done when:

- generated clients can distinguish target metadata messages from data chunk
  messages.
- server validation rejects missing metadata, duplicate metadata, and target
  switches with clear errors.
- existing CLI and SDK write paths continue to work through the new schema.

Completed:

- Replaced path/job-id-in-first-chunk stream messages with explicit
  `target`/`chunk` oneof envelopes for `WriteFile` and `WriteJobStdin`.
- Updated daemon validation and CLI/SDK chunk producers for the new envelope
  shape.
- Documented the target-first streaming contract in PROTOCOL.md.

## Phase 32.27: List Pagination Contract

Status: Completed.

Goal: add explicit pagination fields to list APIs before list results become
large or UI-driven.

Planned:

- add `page_size` and `page_token` to list request messages for capabilities,
  jobs, services, and audit.
- add `next_page_token` to list responses where pagination is meaningful.
- define default and maximum page sizes.
- update daemon handlers to apply pagination deterministically.
- update CLI and SDK list helpers to request all pages by default unless a
  lower-level paginated method is explicitly used.

Done when:

- large audit/job/capability/service lists can be paged through the protocol.
- current CLI behavior still shows complete lists by default.
- PROTOCOL.md documents pagination semantics.

Completed:

- Added `page_size`, `page_token`, and `next_page_token` to capability, job,
  service, and audit list APIs.
- Implemented deterministic daemon pagination and high-level CLI/SDK page
  walking.
- Added Rust and TypeScript tests for pagination behavior.

## Phase 32.28: v0.6.8 Acceptance

Status: Completed.

Goal: make the protocol schema cleanup reproducible.

Planned:

- create `docs/plan/v0.6.8-acceptance.md`.
- regenerate Rust and TypeScript protocol bindings.
- update PROTOCOL.md for typed enums, optional presence, streaming envelopes,
  and pagination.
- update AGENTS.md after completion so the next planned milestone is clear.

Done when:

- `cargo fmt --check` passes.
- `cargo clippy --workspace --locked -- -D warnings` passes.
- `cargo test --workspace --locked` passes.
- `pnpm --filter @operon/sdk typecheck` passes.
- `pnpm --filter @operon/sdk test` passes.
- runtime smoke validation still passes for fs write, job stdin, job logs, and
  list APIs.

Completed:

- Added `docs/plan/v0.6.8-acceptance.md`.
- Regenerated Rust and TypeScript protocol bindings from the stabilized schema.
- Updated PROTOCOL.md, README-adjacent architecture docs, and AGENTS.md for the
  completed milestone.
- Verified format, clippy, Rust tests, SDK typecheck/tests, Docker runtime
  smoke, and v0.6.7 runtime validation.

## Phase 32.29: v0.6.8 Release Cleanup

Status: Completed.

Goal: close final review findings before tagging v0.6.8.

Planned:

- bound runtime-only job maps so completed jobs do not grow daemon memory
  without limit.
- preserve audit context across spawned job task boundaries.
- add current runtime/schema smoke validation to CI and README.
- replace stale daemon flag references with unified `config.yaml` guidance.
- align public protocol version and release examples with v0.6.8.

Done when:

- completed job event broadcasters are removed after terminal events.
- completed job log buffers have a global in-memory retention limit.
- job async tasks run under the captured request audit context.
- CI runs the current runtime validation script.
- README, PROTOCOL.md, and runtime architecture docs match the unified config
  model.
- `PROTOCOL_VERSION` and release instructions point to v0.6.8.

Completed:

- Added bounded completed-job log buffer retention and event broadcaster cleanup.
- Captured request audit context before spawning job execution tasks.
- Expanded `scripts/verify-v0.6.7-runtime.sh` and added it to CI.
- Updated README, PROTOCOL.md, runtime architecture docs, and
  `docs/plan/v0.6.8-release-cleanup.md`.
- Updated `PROTOCOL_VERSION` to `v0.6.8`.

## Phase 32.30: v0.6.9 CLI Contract Cleanup

Status: Completed.

Goal: make the CLI reliable as the supported human, ops, and script interface.

Planned:

- make non-detached `operon --json job run` emit one valid JSON document.
- make non-detached `operon job run` return non-zero for failed, cancelled, or
  timed-out remote jobs.
- make `operon job logs` honor global `--json` and `--quiet`.
- apply `audit show` filters consistently in JSON and text output.
- align daemon health version reporting with the public protocol/release line.
- make `operon init config` generate the referenced starter token and secrets
  files.
- add CLI contract smoke validation to CI.

Done when:

- CLI script output is valid JSON where `--json` is requested.
- quiet mode suppresses log output without bypassing command errors.
- job failures are visible to shell scripts through process exit status.
- starter config files can launch `operond start` without missing-file errors.
- CI runs `scripts/verify-v0.6.9-cli-contract.sh`.

Completed:

- Added `docs/plan/v0.6.9-cli-contract-cleanup.md`.
- Changed non-detached `operon --json job run` to emit only the terminal job
  record.
- Made non-detached `operon job run` return a non-zero CLI error when the
  remote job fails, is cancelled, or times out.
- Made `operon job logs` honor `--json` and `--quiet`.
- Applied `audit show` filters before both JSON and text rendering.
- Updated daemon health to report `PROTOCOL_VERSION`.
- Made `operon init config` generate referenced starter `token` and
  `secrets.yaml` files.
- Added unit tests and `scripts/verify-v0.6.9-cli-contract.sh`, and wired the
  script into CI.

## Phase 32.31: v0.6.10 Runtime Hardening

Status: Completed.

Goal: harden the runtime issues validated after v0.6.9.

Planned:

- `docs/plan/v0.6.10-runtime-hardening.md`.
- make JSONL store appends use secure file handling and sync completed writes.
- reject unsafe daemon store paths outside the config directory or through
  symlinks/special files.
- add terminal job audit events and include real spawn errors in job stderr.
- validate fs range write chunk size, offset overflow, and maximum file bounds.
- align `mkdir` behavior with other fs mutation RPCs.
- preserve `next_page_token` in Rust core list models and protocol conversions.
- handle mDNS removal events during one-shot LAN discovery and surface receiver
  failures.

Done when:

- the validated hardening issues are fixed or explicitly bounded by tests.
- Rust workspace tests cover the key regressions.
- workspace validation passes before commit.

Completed:

- Added `docs/plan/v0.6.10-runtime-hardening.md`.
- Hardened JSONL store appends with secure file opening and sync after record
  writes.
- Rejected daemon store paths outside the config directory and symlink/special
  store targets.
- Added job terminal audit events with run/step context.
- Logged concrete spawn errors into job stderr.
- Added fs write-range chunk, overflow, and maximum object size validation.
- Made `MkdirFs` create missing parent directories.
- Preserved `next_page_token` in Rust core list models and protocol
  conversions.
- Handled mDNS `ServiceRemoved` events in one-shot LAN discovery and surfaced
  receiver failures.
- Added focused tests plus `scripts/verify-v0.6.10-runtime-hardening.sh` to CI.

Remaining:

- No open v0.6.10 items.

## Phase 32.32: v0.6.11 Maintainability Governance

Status: Completed.

Goal: reduce the highest-risk maintenance issues before starting larger feature
work.

Planned:

- `docs/plan/v0.6.11-maintainability-governance.md`.
- split daemon defaults, LAN advertise, store-path validation, status mapping,
  and lock handling out of `operond/src/main.rs`.
- make gRPC-facing daemon lock acquisition return `Status::internal` instead of
  panicking on poisoned mutexes.
- make Linux-only mount support explicit through target-specific dependencies
  and a non-Linux CLI error path.
- add focused validation coverage for the governance checks.

Done when:

- the high-risk daemon helper areas have module boundaries.
- gRPC request paths no longer use direct poisoned-lock `expect` handling for
  shared runtime state.
- non-Linux builds are not forced to compile `operon-mount`.
- CI runs `scripts/verify-v0.6.11-governance.sh`.
- workspace validation passes.

Completed:

- Added `docs/plan/v0.6.11-maintainability-governance.md`.
- Split `operond` support code into `defaults`, `grpc_status`,
  `lan_advertise`, `locks`, and `store_config` modules.
- Removed direct poisoned-lock `expect` calls from `operond/src/main.rs`.
- Added a gRPC lock helper that maps poisoned shared-state locks to
  `Status::internal`.
- Changed background job/audit cleanup paths to log poisoned locks and return
  instead of panicking.
- Made `operon-cli` depend on `operon-mount` only on Linux targets.
- Added a non-Linux `operon mount` unsupported-platform error path.
- Added `scripts/verify-v0.6.11-governance.sh` and wired it into CI.

Remaining:

- Larger domain splits remain future work: `operond` server/fs/job/audit
  modules, `operon-cli` command modules, and `operon-mount` remote/inode/FUSE
  modules.

## Phase 32.33: v0.6.12 Runtime Boundary Stabilization

Status: Completed.

Goal: stabilize the long-term protocol, store, job log stream, daemon runtime
helper, and mount adapter boundaries before v0.7 service forwarding work.

Planned:

- `docs/plan/v0.6.12-runtime-boundary-stabilization.md`.
- replace bare `StreamJobLogs` chunks with a typed streaming envelope that can
  carry snapshots, entries, and terminal metadata.
- keep `ListJobLogs` as the snapshot query API while CLI and SDK consume the
  streaming envelope for live log flows.
- promote `operon-store` to an explicit append-only event writer boundary with
  visible fsync policy and `Result`-returning append operations.
- surface store append failures at daemon runtime boundaries.
- consolidate daemon background job/log/audit lock handling through runtime
  helper boundaries instead of scattered `eprintln!` paths.
- make `operon-mount` a Linux FUSE adapter boundary by excluding the `fuser`
  dependency outside Linux.
- add focused validation coverage and wire it into CI.

Done when:

- `StreamJobLogs` returns envelope messages.
- CLI JSON and stream output preserve job-log truncation metadata.
- TS SDK exposes real stream events for job logs.
- `operon-store` append failures are testable and no longer swallowed inside the
  store crate.
- daemon persistence failures are logged consistently at the daemon boundary.
- non-Linux builds do not resolve `fuser` through `operon-mount`.
- CI runs `scripts/verify-v0.6.12-runtime-boundary.sh`.
- workspace validation passes.

Completed:

- Added `docs/plan/v0.6.12-runtime-boundary-stabilization.md`.
- Replaced raw `StreamJobLogs` chunks with a `JobLogStreamEvent` envelope that
  carries snapshot, entry, and complete variants.
- Updated daemon streaming to emit initial snapshots, live entries, lag
  snapshots, and terminal completion metadata.
- Updated CLI JSON and streaming log paths to consume the envelope and preserve
  truncation metadata.
- Updated the TypeScript SDK generated client, public stream event types, byte
  stream helper, and SDK tests for the new envelope.
- Added `StoreWriter` and `FsyncPolicy` to `operon-store`; append failures now
  return `Result`.
- Routed daemon append-only persistence through the store writer boundary and
  logged persistence failures at daemon runtime boundaries.
- Replaced remaining background mutex-poison `eprintln!` paths in daemon
  runtime helpers with structured tracing errors.
- Made `operon-mount` a Linux-only FUSE adapter boundary by gating the crate and
  the `fuser` dependency to Linux.
- Updated protocol docs, runtime architecture docs, README release examples,
  and the public protocol version to v0.6.12.
- Completed a post-release documentation drift pass that aligned current docs
  with v0.6.12 and marked older acceptance docs as historical snapshots.
- Added `scripts/verify-v0.6.12-runtime-boundary.sh` and wired it into CI.
- Validation passed:
  - `scripts/verify-v0.6.12-runtime-boundary.sh`
  - `scripts/verify-v0.6.7-runtime.sh`
  - `scripts/verify-v0.6.9-cli-contract.sh`
  - `scripts/verify-v0.6.10-runtime-hardening.sh`
  - `scripts/verify-v0.6.11-governance.sh`
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --locked -- -D warnings`
  - `cargo test --workspace --locked`
  - `pnpm typecheck`
  - `pnpm test`

Remaining:

- No open v0.6.12 implementation items.

## v0.7 Goal

Operon v0.7 should complete the service capability with explicit local port
forwarding for policy-allowed services.

```text
v0.7 = service metadata + health checks + explicit local forwarding.
```

The CLI TUI console phase is cancelled. Operon should stay CLI/SDK/protocol
first and avoid adding a separate interactive console surface for now.

Service forwarding is intentionally narrow: the client opens a local listener
and tunnels each accepted TCP connection through an already reachable Operon
daemon connection to a service configured in daemon policy. It must not become
VPN behavior, relay networking, NAT traversal, global routing, or unmanaged
port exposure.

## Phase 33: Service Forwarding Protocol

Status: Completed.

Goal: add an explicit runtime protocol for forwarding policy-allowed local
services.

Completed:

- added `OpenServiceTunnel` as a bidirectional gRPC stream.
- kept `ListServices` and `CheckService` as service metadata and health-check
  APIs.
- kept the policy boundary at configured `service.services` entries.
- audited allowed and denied service forwarding attempts.

Done when:

- direct protocol clients can tunnel bytes without using an SDK.
- the schema makes the initial target envelope explicit.

## Phase 34: CLI and SDK Service Forwarding

Status: Completed.

Goal: expose service forwarding through supported clients without reintroducing
an HTTP facade.

Completed:

- added `operon service forward <node-id> <service-id> --listen <addr>`.
- each accepted local TCP connection opens one runtime service tunnel.
- TypeScript SDK exposes `openServiceTunnel` over `nice-grpc`.
- existing command-style service list/check commands remain available for
  scripts and CI.

Done when:

- node B can expose `127.0.0.1:8080` locally and tunnel to node A's configured
  `127.0.0.1:80` service over an already reachable Operon node connection.
- `operon --json` still works for service metadata and tunnel startup status.

## Phase 35: Service Forwarding Validation

Status: Completed.

Goal: make local forwarding reproducible in CI.

Completed:

- added SDK tests for service tunnel request/response streaming.
- added `scripts/verify-v0.7-service-forwarding.sh` with a local HTTP service,
  daemon policy, CLI forwarding, HTTP fetch through the forwarded port, and
  audit validation.
- wired the v0.7 validation script into CI.

Done when:

- service forwarding is covered by focused SDK tests and runtime smoke
  validation.
- docs explain that forwarding is explicit and policy controlled.

## Phase 36: v0.7 Acceptance

Status: Completed.

Goal: make the service forwarding milestone reproducible.

Completed:

- `docs/plan/v0.7-acceptance.md`.
- README roadmap update.
- protocol and architecture docs update.
- CI-backed local service forwarding smoke path.

Done when:

- v0.7 has documented acceptance criteria.
- the roadmap no longer contains TUI console work.
- service capability explicitly includes metadata, health checks, and local
  forwarding.

## v0.7.1 Goal

Operon v0.7.1 adds UDP/datagram forwarding as a separate protocol and
runtime phase from v0.7 TCP forwarding.

```text
v0.7.1 = explicit UDP/datagram forwarding for policy-allowed services.
```

UDP must not be folded into the existing `OpenServiceTunnel` TCP byte stream.
Datagram forwarding needs packet-boundary preservation, peer-session handling,
idle cleanup, packet-size behavior, and separate audit semantics.

## Phase 37: UDP / Datagram Forwarding Design

Status: Completed.

Goal: define the UDP forwarding contract before implementation.

Completed:

- create and maintain `docs/plan/v0.7.1-udp-datagram-forwarding.md`.
- represented UDP services through `ServiceProtocol::Udp`.
- defined `OpenServiceDatagramTunnel` request/response envelopes and
  packet-boundary semantics.
- defined local UDP listener behavior and peer session expiration.
- defined audit action `forward-udp`.
- defined explicit non-goals: NAT traversal, UDP hole punching, relay
  networking, mDNS relay, global routing, and arbitrary host/port forwarding.

Done when:

- UDP/datagram forwarding semantics are documented.
- TCP forwarding remains unchanged.
- the protocol does not reuse `OpenServiceTunnel` for datagram traffic.

## Phase 38: UDP / Datagram Forwarding Implementation

Status: Completed.

Goal: implement policy-controlled UDP forwarding over existing Operon node
connections.

Completed:

- added datagram-oriented gRPC API `OpenServiceDatagramTunnel`.
- extended service policy and protocol types for UDP without weakening TCP
  behavior.
- implemented daemon-side UDP socket forwarding only to configured services.
- added `operon service forward-udp`.
- exposed TypeScript SDK datagram tunnel helpers over `nice-grpc`.

Done when:

- a local UDP client can send datagrams through Operon to a policy-allowed
  daemon-local UDP service.
- packet boundaries are preserved.
- denied UDP service ids fail through policy and audit.

## Phase 39: v0.7.1 Acceptance

Status: Completed.

Goal: make UDP/datagram forwarding reproducible.

Completed:

- added `scripts/verify-v0.7.1-udp-datagram-forwarding.sh`.
- validated against a local UDP echo service.
- updated README, PROTOCOL.md, architecture docs, and AGENTS.md.

Done when:

- v0.7.1 has documented acceptance criteria.
- CI validates UDP datagram forwarding.
- docs clearly distinguish TCP byte-stream forwarding from UDP datagram
  forwarding.

## v0.8 Goal

Operon v0.8 should ship an agent skills pack after the gRPC runtime, Linux
mount, and service forwarding are stable.

```text
v0.8 = skills that teach agents how to use Operon.
```

v0.8 should not add MCP, a separate agent runtime, or a parallel control plane.
The deliverable is documentation-as-behavior: portable skills that teach an
agent to use the existing `operon` CLI, config model, protocol docs, and
TypeScript SDK safely.

## Phase 40: Agent Skill Contract

Status: Completed.

Goal: define the skills agents will load.

Completed:

- reviewed CLI help coverage and improved high-value help text for commands that
  agents will call directly.
- added a CLI config interpretation view, `operon config explain`, so the
  CLI can summarize the active config without requiring agents to understand
  raw YAML first.
- chose the repo-local skill layout `skills/<name>/SKILL.md`.
- defined a small skills pack for:
  - Operon concepts and safety boundaries.
  - config and onboarding.
  - CLI node/capability/fs/job/service/audit/trace workflows.
  - service forwarding, including TCP and UDP differences.
  - direct protocol and SDK usage for agents that need code-level integration.
- defined skill frontmatter, trigger descriptions, prerequisites, and examples.
- kept skills focused on scenarios, decision paths, safety rules, and which
  commands to use; skills should tell agents to inspect `operon <command>
  --help` for exact flags instead of duplicating the CLI manual.
- documented destructive-operation rules, including explicit confirmation for
  writes, deletes, job execution, cancellation, and forwarding commands.

Done when:

- every public CLI command exposes working help output.
- the config interpretation view explains daemon, client nodes, auth sources,
  policy scopes, service definitions, secrets references, and default config
  path behavior.
- the skill set is documented.
- each skill points agents to existing runtime APIs instead of inventing new
  surfaces.
- audit and trace semantics remain unchanged.

## Phase 41: Agent Skills Implementation

Status: Completed.

Goal: create the skills that teach agents how to operate Operon.

Completed:

- added the CLI config interpretation command and included `--json` output.
- added repo-local skill directories and `SKILL.md` files.
- included scenario-oriented CLI examples for common workflows.
- included config file expectations and default config path behavior.
- included policy-aware guidance for fs, jobs, service forwarding, audit, and
  trace usage.
- included instructions for agents to call the relevant `--help` command before
  using less familiar flags or subcommands.
- included SDK/protocol notes only where CLI usage is insufficient.
- included example agent playbooks for inspection, controlled fs/job workflows,
  and service forwarding checks.

Done when:

- `operon config explain` or the chosen equivalent gives agents a safe summary
  of the active `config.yaml`.
- an agent with the skills can inspect nodes and capabilities through `operon`.
- an agent with the skills can run a constrained workflow without bypassing
  policy.
- the skills teach agents to check audit and trace output after actions.

## Phase 42: v0.8 Acceptance

Status: Completed.

Goal: make the skills pack reproducible.

Completed:

- `docs/plan/v0.8-acceptance.md`.
- CLI help and config explain validation.
- static validation for skill structure and required safety sections.
- smoke validation that documented CLI examples match current command names.
- README updates documenting v0.8 validation and skills status.

Done when:

- v0.8 has documented acceptance criteria.
- the skills use existing runtime contracts instead of introducing a parallel
  control plane.

## v0.9 Goal

Operon v0.9 should make endpoint-only configuration and mDNS discovery
reproducible while preserving the network boundary.

```text
v0.9 = endpoint model acceptance and mDNS discovery UX.
```

v0.9 should consume explicit endpoints and optionally discover local mDNS
endpoint candidates. It must not implement NAT traversal, relays, VPN behavior,
mesh IP assignment, subnet routing, global routing, or provider-specific API
adapters.

## Phase 43: CLI Shell Completion Cleanup

Status: Completed.

Goal: make the CLI easier to use interactively before endpoint discovery UX
work.

Completed:

- added `operon completion <shell>` using generated completions from the clap
  command model.
- supported bash and zsh completion generation, with the same command also
  available for other clap-supported shells.
- added completion setup guidance to the `operon onboard` flow without directly
  mutating shell startup files.
- extended v0.8 validation to cover completion help, bash generation, zsh
  generation, and onboard completion guidance.

Done when:

- `operon completion bash` generates a bash completion script.
- `operon completion zsh` generates a zsh completion script.
- `operon onboard` shows the user how to install completions.

## Phase 44: Test Coverage and Integration Audit

Status: Completed.

Goal: make test coverage explicit and add integration tests before endpoint
discovery UX expands the surface area.

Completed:

- audited unit coverage across every Rust crate and recorded the current
  coverage map in `docs/quality/test-coverage-audit.md`.
- added compiled-binary CLI integration tests for help, shell completions,
  starter config generation, `config explain --json`, and onboard completion
  guidance.
- added `scripts/verify-v0.8.1-integration-coverage.sh`, which starts a real
  daemon and exercises config, node, capability, fs, job, service, audit,
  graph, trace, and completion flows.
- added the integration coverage validation script to CI.

Done when:

- every Rust crate has registered unit tests.
- CLI binary behavior is covered outside helper-only unit tests.
- a real daemon integration smoke validates the core user-facing flows.
- test coverage expectations are documented for future phases.

## Phase 45: Runtime Cleanup and Hardening Triage

Status: Completed.

Goal: resolve concrete cleanup findings from code review and record larger
policy/protocol hardening items for later phases.

Completed:

- changed audit timestamps to use `u64` end-to-end across `operon-core` and
  `operon-protocol`, matching the gRPC `uint64` schema.
- extracted shared CLI private-file and token helpers used by `init config` and
  `onboard`.
- updated UDP service forwarding cleanup so the local socket read task is
  aborted and awaited.
- added explicit service action permissions and enforced `check` / `forward`
  authorization separately.
- documented deferred config default plus protocol empty chunk, tunnel close
  semantics, and daemon state ownership items in
  `docs/plan/v0.8.2-runtime-cleanup.md`.

Done when:

- review findings for timestamp conversion, duplicate token generation,
  duplicate private-file handling, and UDP abort cleanup are resolved.
- service check and forwarding are authorized through explicit action
  permissions.
- deferred hardening items are explicit and not hidden as incidental TODOs.
- focused tests cover the changed conversion and helper behavior.

## Phase 46: v0.8.3 Read Range and Release Cleanup

Status: Completed.

Goal: close the concrete FUSE random-read performance gap and make release,
package, and protocol version rules explicit before endpoint discovery UX.

Plan:

- add `ReadFileRange(path, offset, size)` to the gRPC runtime protocol.
- implement daemon range reads with direct seek-and-read behavior.
- update the Linux FUSE mount adapter so `read_range` no longer streams the
  full file and skips bytes locally.
- update protocol/core conversions, TS SDK generation, and focused tests for
  the new range-read API.
- keep `ReadFile` as the streaming full-file API and document the difference.
- clean up README release examples so they do not hard-code a stale release
  version.
- document how GitHub release tags, Rust crate versions, TS SDK package
  versions, and `PROTOCOL_VERSION` relate to each other.

Done when:

- FUSE random reads use `ReadFileRange`.
- daemon range-read validation prevents offset/size overflow.
- protocol and SDK tests cover the new API surface.
- README and release docs do not imply `v0.6.12` is the current install target.
- version policy explains why protocol version bumps are tied to wire/API
  compatibility, not every skills, testing, or internal cleanup phase.

Detailed plan: `docs/plan/v0.8.3-read-range-release-cleanup.md`.

Completed:

- Added `ReadFileRange` to the gRPC runtime protocol.
- Implemented daemon direct range reads and audit events.
- Updated Linux FUSE `GrpcRemoteFs::read_range` to use `ReadFileRange`.
- Added SDK range-read helper and regenerated TypeScript proto bindings.
- Bumped `PROTOCOL_VERSION` to `v0.8.3`.
- Updated README, `PROTOCOL.md`, runtime architecture docs, and CI validation
  for release/package/protocol version policy.
- Validation passed with
  `scripts/verify-v0.8.3-read-range-release-cleanup.sh`.

## Phase 47: v0.8.4 Runtime and CLI Modularization

Status: Completed.

Goal: reduce the largest maintenance hotspots through behavior-preserving
module splits before adding endpoint discovery UX.

Plan:

- split `crates/operond/src/main.rs` so it keeps startup wiring and top-level
  command dispatch, while fs, job, service forwarding, audit, pagination, and
  runtime state move into focused modules.
- split `crates/operon-cli/src/main.rs` so it keeps clap model construction and
  high-level dispatch, while command families, output rendering, and target
  parsing move into focused modules.
- preserve current public CLI behavior, gRPC behavior, JSON output, quiet
  output, and failure exit semantics.
- add focused module-level tests where extraction exposes pure helpers.

Done when:

- `operond/src/main.rs` no longer directly owns fs, job, service-forwarding,
  audit, and pagination implementation details.
- `operon-cli/src/main.rs` no longer directly owns every command handler and
  renderer.
- existing daemon, CLI, service, mount, SDK, and integration validations remain
  green.
- intentionally deferred extraction is documented with an owner module and
  reason.

Detailed plan: `docs/plan/v0.8.4-runtime-cli-modularization.md`.

Completed:

- Extracted daemon filesystem runtime handlers into `fs_service`.
- Extracted daemon pagination helpers into `pagination`.
- Extracted CLI output helpers into `output`.
- Extracted CLI target parsing and endpoint loading into `target`.
- Extracted CLI filesystem command handlers into `commands/fs`.
- Added CI validation for the current modularization boundaries.
- Validation passed with `scripts/verify-v0.8.4-modularization.sh`.

Remaining:

- Job runtime, service forwarding, audit helpers, and non-fs CLI command
  families still need follow-up extraction before major feature work in those
  areas.

## Phase 48: v0.8.5 Core Domain Module Boundaries

Status: Completed.

Goal: split `operon-core` into domain modules before endpoint discovery UX and
policy/trace schemas grow further.

Plan:

- move runtime identity, capability, and health DTOs into
  `operon_core::runtime`.
- move filesystem DTOs into `operon_core::fs`.
- move job DTOs into `operon_core::job`.
- move service DTOs and service policy definitions into
  `operon_core::service`.
- move policy roots and fs/job policy definitions into
  `operon_core::policy`.
- move audit DTOs into `operon_core::audit`.
- move discovery DTOs into `operon_core::discovery`.
- move execution graph and trace DTOs into `operon_core::trace`.
- keep root-level `pub use` re-exports so existing callers remain compatible.

Done when:

- `crates/operon-core/src/lib.rs` only wires modules, re-exports public types,
  and keeps crate-level tests.
- serialized YAML/JSON names, gRPC schemas, SDK APIs, CLI behavior, and daemon
  behavior do not change.
- full Rust validation remains green.

Detailed plan: `docs/plan/v0.8.5-core-domain-module-boundaries.md`.

Completed:

- Split `operon-core` into `runtime`, `fs`, `job`, `service`, `policy`,
  `audit`, `discovery`, and `trace` modules.
- Kept root-level public re-exports so current downstream imports continue to
  work.
- Preserved serde formats, gRPC schemas, SDK APIs, CLI behavior, and daemon
  behavior.
- Added module path / root re-export coverage in `operon-core` tests.
- Added `scripts/verify-v0.8.5-core-domain-modules.sh` and wired it into CI.

Remaining:

- No v0.8.5 work remains.
- Moving policy or discovery into separate crates remains a future decision
  only if module boundaries stop being enough.

## Phase 49: v0.8.6 Runtime, CLI, and Client Modularization

Status: Completed.

Goal: finish the deferred maintainability split before endpoint discovery UX by
moving daemon job/service/audit/log internals, non-fs CLI command families, and
shared Rust gRPC client concerns behind focused module boundaries.

Plan:

- split `operond` runtime internals into state, runtime service, auth, job
  runtime, job logs, service forwarding, datagram forwarding, and audit
  modules.
- split non-fs `operon-cli` command families into `commands/*` modules and
  reduce repeated text/json/quiet rendering branches where practical.
- add a lightweight Rust `operon-grpc-client` crate for tonic endpoint URI
  normalization, auth/context metadata, typed client construction, and Rust-side
  stream chunk helpers shared by CLI and mount.
- split `operon-mount` into remote client, inode table, FUSE callbacks, path,
  errors, and session modules while keeping it a Linux adapter crate.
- add `operon graph run` and optionally `operon workflow run` aliases while
  keeping top-level `operon run` compatible.
- make `operon --json fs read <target> --output <file>` return a structured
  write summary.
- expose direct TypeScript SDK methods for `statFs`, `listFs`, `runJob`,
  `getJob`, `cancelJob`, `listCapabilities`, and `listAudit`.
- extract low-risk validation shell helpers for daemon startup, cleanup,
  temporary config setup, and `wait_for_node`.

Done when:

- `crates/operond/src/main.rs` no longer directly owns job runtime, job log
  retention, audit append, TCP service tunnel, or UDP datagram tunnel internals.
- `crates/operon-cli/src/main.rs` no longer owns non-fs command handlers and
  renderers.
- CLI and mount share Rust gRPC endpoint/auth/client helpers.
- `operon-mount` has module boundaries for remote client, inode table, FUSE
  callbacks, paths, errors, and session lifecycle.
- TypeScript SDK exposes direct public methods for the listed core protocol
  capabilities.
- behavior-sensitive CLI/SDK/script contracts remain green.

Detailed plan:
`docs/plan/v0.8.6-runtime-cli-client-modularization.md`.

Completed:

- Added `operon-grpc-client` and migrated CLI plus Linux mount gRPC callers to
  shared endpoint/auth/context/client/chunk helpers.
- Split non-fs CLI command handlers into `commands/*` modules and reduced
  `operon-cli/src/main.rs` to Clap model construction and high-level dispatch.
- Added `operon graph run` and `operon workflow run` aliases while preserving
  top-level `operon run`.
- Updated `operon --json fs read <target> --output <file>` to emit a
  structured `{ path, output, bytes_written }` summary.
- Split Linux mount internals into remote client, inode table, FUSE callbacks,
  path, errors, and session modules.
- Split daemon auth, audit, state, job runtime/log retention, and service
  forwarding internals out of `operond/src/main.rs`.
- Exposed direct TypeScript SDK methods for capabilities, fs stat/list, job
  run/get/cancel, and audit listing.
- Added reusable validation helpers in `scripts/lib/validation.sh`.
- Added `scripts/verify-v0.8.6-runtime-cli-client-modularization.sh` and wired
  it into CI.

Remaining:

- No blocking v0.8.6 work remains.
- Moving the tonic `GrpcRuntime` routing impl into a separate
  `runtime_service.rs` remains an optional future cleanup if method routing
  grows again.

## Phase 50: v0.8.7 Filesystem Service Reuse Cleanup

Status: Completed.

Goal: reduce repeated filesystem authorization, path resolution, and audit
denial handling in the daemon filesystem service.

Review finding:

- `crates/operond/src/fs_service.rs` repeated the same `authorize_fs`, path
  resolver, failed audit event, and `tonic::Status` conversion pattern across
  most filesystem operations.
- That repetition made the fs service harder to review and increased the risk
  that future operations would drift in permission or audit behavior.

Done when:

- filesystem authorization denial handling has one helper boundary.
- workspace path resolution failures are audited through focused helpers.
- the existing fs operation permissions, audit action names, audit resources,
  and success audit behavior remain unchanged.
- validation guards the helper boundary and daemon tests remain green.

Detailed plan:
`docs/plan/v0.8.7-fs-service-reuse-cleanup.md`.

Completed:

- Added `authorize_fs_action` plus focused workspace path resolver helpers in
  `crates/operond/src/fs_service.rs`.
- Reused those helpers across stat, list, read range, write range, truncate,
  mkdir, delete, rename, and copy operations.
- Added `scripts/verify-v0.8.7-fs-service-reuse-cleanup.sh`.

Remaining:

- No v0.8.7 work remains.
- Moving the full tonic `GrpcRuntime` trait implementation out of
  `operond/src/main.rs` remains a future maintainability candidate if runtime
  method routing grows again.

## Phase 51: v0.8.8 Filesystem Stream Handler Cleanup

Status: Completed.

Goal: keep full-file filesystem stream behavior inside the daemon filesystem
service module instead of the tonic runtime router.

Review finding:

- `crates/operond/src/main.rs` still owned full-file `ReadFile` and
  `WriteFile` authorization, workspace path resolution, audit failure handling,
  chunk-size validation, and file IO.
- That duplicated the filesystem service boundary improved in v0.8.7 and kept
  filesystem business logic in the gRPC router.

Done when:

- `fs_service.rs` owns full-file read and write stream handlers.
- `operond/src/main.rs` only performs gRPC auth, audit context scoping, and
  delegation for `ReadFile` and `WriteFile`.
- validation guards against reintroducing stream handler logic in `main.rs`.
- daemon tests remain green.

Detailed plan:
`docs/plan/v0.8.8-fs-stream-handler-cleanup.md`.

Completed:

- Added `fs_service::read_stream` and `fs_service::write_stream`.
- Reused the v0.8.7 authorization and path resolution helpers for full-file
  stream reads and writes.
- Reduced the `ReadFile` and `WriteFile` runtime methods to delegation.
- Added `scripts/verify-v0.8.8-fs-stream-handler-cleanup.sh`.

Remaining:

- No v0.8.8 work remains.
- Moving the full tonic `GrpcRuntime` trait implementation out of
  `operond/src/main.rs` remains a future maintainability candidate if runtime
  method routing grows again.

## Phase 52: v0.8.9 Service Tunnel Boundary Cleanup

Status: Completed.

Goal: keep service tunnel target parsing, authorization, protocol checks, audit
handling, and connection setup inside the daemon service forwarding module.

Review finding:

- `crates/operond/src/main.rs` still owned TCP and UDP service tunnel open
  handshakes: target-envelope validation, service policy authorization,
  protocol mismatch checks, audit records, TCP connection setup, and datagram
  stream delegation.
- That kept service forwarding business logic in the gRPC router instead of
  behind `service_forward.rs`.

Done when:

- `service_forward.rs` owns TCP and UDP tunnel open/handshake logic.
- `operond/src/main.rs` only performs gRPC auth, audit context scoping, and
  delegation for service tunnel RPCs.
- validation guards against reintroducing tunnel handshake logic in `main.rs`.
- daemon tests remain green.

Detailed plan:
`docs/plan/v0.8.9-service-tunnel-boundary-cleanup.md`.

Completed:

- Added `service_forward::open_service_tunnel` and
  `service_forward::open_service_datagram_tunnel`.
- Added service tunnel stream type aliases for the runtime trait associated
  stream types.
- Reduced service tunnel runtime methods to delegation.
- Added `scripts/verify-v0.8.9-service-tunnel-boundary-cleanup.sh`.

Remaining:

- No v0.8.9 work remains.
- Moving the full tonic `GrpcRuntime` trait implementation out of
  `operond/src/main.rs` remains a future maintainability candidate if runtime
  method routing grows again.

## Phase 53: v0.8.10 Mount Lock Hardening

Status: Completed.

Goal: make Linux FUSE mount callbacks return filesystem errors instead of
panicking when the inode table lock is poisoned.

Review finding:

- `crates/operon-mount/src/fuse_fs.rs` used repeated
  `expect("inode table poisoned")` calls inside production FUSE callbacks.
- A poisoned inode-table lock could panic the mount process instead of
  returning a normal errno to the kernel.

Done when:

- inode-table write lock acquisition has a focused helper boundary.
- FUSE callbacks convert inode-table write lock failures into errno replies or
  propagated mount errors.
- validation rejects reintroducing direct inode-table lock panics.
- mount crate tests remain green.

Detailed plan:
`docs/plan/v0.8.10-mount-lock-hardening.md`.

Completed:

- Added `write_inodes` in `crates/operon-mount/src/fuse_fs.rs`.
- Replaced direct write-lock `expect` calls across lookup/upsert, setattr,
  unlink, rmdir, rename, write cache refresh, and readdir paths.
- Added `scripts/verify-v0.8.10-mount-lock-hardening.sh`.

Remaining:

- No v0.8.10 work remains.
- Broader Linux mount callback decomposition remains a future candidate if the
  FUSE adapter grows beyond a thin adapter boundary.

## Phase 54: v0.8.11 CLI Datagram Lock Hardening

Status: Completed.

Goal: make CLI UDP/datagram forwarding report peer-state lock failures instead
of panicking.

Review finding:

- `crates/operon-cli/src/grpc.rs` used
  `expect("datagram peer state poisoned")` in UDP datagram forwarding peer
  state helpers.
- A poisoned peer-state lock could panic a long-running `operon service
  forward-udp` process instead of returning a normal CLI error.

Done when:

- datagram peer-state helpers return `anyhow::Result`.
- inbound peer lookup and removal failures propagate through the forwarding
  command path.
- validation rejects reintroducing datagram peer-state lock panics.
- CLI tests remain green.

Detailed plan:
`docs/plan/v0.8.11-cli-datagram-lock-hardening.md`.

Completed:

- Changed datagram peer-state helpers to return `anyhow::Result`.
- Propagated inbound peer lookup and removal failures through the forwarding
  command path.
- Changed local UDP read task lock failures to stop forwarding instead of
  panicking the task.
- Added `scripts/verify-v0.8.11-cli-datagram-lock-hardening.sh`.

Remaining:

- No v0.8.11 work remains.
- Broader `operon-cli/src/grpc.rs` command-family split remains a future
  maintainability candidate.

## Phase 55: v0.8.12 Daemon Datagram Invariant Cleanup

Status: Completed.

Goal: remove the remaining production invariant panic from daemon UDP/datagram
forwarding.

Review finding:

- `crates/operond/src/service_forward.rs` used
  `expect("session should exist after creation")` after creating or looking up
  a UDP peer session.
- The condition should be handled as a tunnel close response instead of a
  daemon panic, even if it should be unreachable in normal execution.

Done when:

- service datagram session lookup has an explicit missing-session branch.
- validation rejects reintroducing the session invariant panic.
- daemon tests remain green.

Detailed plan:
`docs/plan/v0.8.12-daemon-datagram-invariant-cleanup.md`.

Completed:

- Replaced the session lookup `expect` with a close response for the affected
  peer.
- Added `scripts/verify-v0.8.12-daemon-datagram-invariant-cleanup.sh`.

Remaining:

- No v0.8.12 work remains.
- Broader service datagram state-machine extraction remains a future candidate
  if UDP forwarding behavior grows.

## Phase 56: v0.8.13 Production Panic Cleanup

Status: Completed.

Goal: remove the production panic-style invariants found in daemon job-log
handling and Linux mount remote client runtime access.

Review finding:

- `crates/operond/src/job_runtime.rs` used
  `expect("just pushed job log")` after appending a job log entry.
- `crates/operon-mount/src/remote_client.rs` used
  `expect("remote fs runtime is only cleared during drop")` when resolving the
  blocking runtime used by remote filesystem operations.
- Both sites should fail as logged errors or returned errors instead of
  panicking production processes.

Done when:

- job-log append handles an unexpectedly empty retained log buffer without a
  panic.
- mount remote runtime lookup failures return normal errors through remote fs
  operations.
- validation rejects reintroducing both production invariant panics.
- daemon and mount tests remain green.

Detailed plan:
`docs/plan/v0.8.13-production-panic-cleanup.md`.

Completed:

- Replaced the job-log append invariant panic with an explicit logged branch.
- Changed the mount remote runtime accessor to return `anyhow::Result`.
- Propagated remote runtime lookup errors through remote filesystem
  operations.
- Added `scripts/verify-v0.8.13-production-panic-cleanup.sh`.

Remaining:

- No v0.8.13 work remains.
- CLI file upload and job stdin helpers still buffer local files before
  sending requests. That is not a panic, but remains a future streaming-client
  improvement candidate for very large local inputs.

## Phase 57: v0.8.14 Onboard Invariant Cleanup

Status: Completed.

Goal: remove the production invariant panic from guided onboarding plan
construction.

Review finding:

- `crates/operon-cli/src/onboard.rs` used
  `expect("daemon onboarding should have a token")` after deriving the daemon
  token for daemon and combined onboarding roles.
- The token should always exist for those roles, but a broken invariant should
  return a normal CLI error instead of panicking.

Done when:

- daemon onboarding token lookup returns a normal error on invariant failure.
- validation rejects reintroducing the onboarding token panic.
- CLI tests remain green.

Detailed plan:
`docs/plan/v0.8.14-onboard-invariant-cleanup.md`.

Completed:

- Replaced the daemon-token `expect` with an explicit `anyhow` error branch.
- Added `scripts/verify-v0.8.14-onboard-invariant-cleanup.sh`.

Remaining:

- No v0.8.14 work remains.
- `operon-cli` still contains test-only assertion panics and one
  `String` formatting invariant in token generation; those do not represent
  user-triggered onboarding panics.

## Phase 58: v0.8.15 Token Generation Panic Cleanup

Status: Completed.

Goal: remove the remaining production panic-style token formatting invariant
from CLI private-file helpers.

Review finding:

- `crates/operon-cli/src/private_files.rs` formatted generated token bytes
  with `write!` and `expect("writing to String should not fail")`.
- Writing to a `String` is effectively infallible, but token generation does
  not need a panic-style assertion for hex encoding.

Done when:

- generated token hex encoding does not use panic-style formatting.
- validation rejects reintroducing the `String` write `expect`.
- CLI tests remain green.

Detailed plan:
`docs/plan/v0.8.15-token-generation-panic-cleanup.md`.

Completed:

- Replaced `write!`-based hex formatting with direct nibble-to-character
  encoding.
- Removed the now-unused `fmt::Write` import.
- Added `scripts/verify-v0.8.15-token-generation-panic-cleanup.sh`.

Remaining:

- No v0.8.15 work remains.
- The remaining `expect`, `unwrap`, and `panic!` scan hits in the reviewed
  Rust surfaces are test assertions.
- CLI file upload and job stdin helpers still buffer local files before
  sending requests. That remains a future streaming-client improvement
  candidate for very large local inputs.

## Phase 59: v0.8.16 Endpoint Model Simplification

Status: Completed.

Goal: remove the provider abstraction from Operon's user-facing endpoint model.

Decision:

- Operon consumes explicit gRPC endpoints. It does not need to know whether an
  endpoint is reachable through Cloudflare Mesh, Tailscale, WireGuard, SSH,
  Kubernetes DNS, LAN, or another private network.
- mDNS remains a convenience mechanism for discovering candidate LAN endpoints,
  not a provider type.
- External network systems solve reachability. Operon starts at
  `node_id -> endpoint` and owns capability policy, execution, audit, and
  traces.

Done when:

- `provider` is removed from `NodeEndpoint`, `NodeConfig`, mDNS discovery
  records, generated config, CLI output, and the TypeScript SDK endpoint type.
- the legacy provider command and discovery provider flag are removed.
- stale `provider` fields in older client node config are ignored rather than
  consumed as model data.
- current docs and acceptance criteria describe endpoint-only configuration.

Detailed plan:
`docs/plan/v0.8.16-endpoint-model-simplification.md`.

Completed:

- Removed `NetworkProviderKind` and provider fields from endpoint/config
  structs.
- Removed provider output from node list/resolve/discover and config explain.
- Removed provider metadata from mDNS advertisement and discovery records.
- Removed the provider CLI command and discover provider flag.
- Updated init/onboard generated config, validation scripts, README, and v0.9
  acceptance docs.
- Left stale `provider` config fields inert so existing endpoint entries are
  not blocked by metadata Operon no longer consumes.
- Added `scripts/verify-v0.8.16-endpoint-model-simplification.sh`.

Remaining:

- No v0.8.16 work remains.
- Future discovery work should improve endpoint import/export and mDNS UX, not
  add provider-specific runtime behavior.

## Phase 60: v0.8.17 Config Unknown Field Warnings

Status: Completed.

Goal: warn about unknown `config.yaml` fields without blocking startup or CLI
commands.

Review finding:

- After the endpoint model simplification, stale fields such as `provider`
  should not be consumed, but rejecting them outright is unnecessary because
  the endpoint entry remains usable.
- Silent ignore is also too loose for configuration hygiene. Operators should
  see which fields are inert.

Done when:

- config parsing collects unknown field paths.
- `OperonConfig::load` warns about unknown field paths and keeps loading.
- stale `provider` fields are reported as unknown but do not block startup or
  CLI commands.
- validation covers config parsing and CLI stderr behavior.

Detailed plan:
`docs/plan/v0.8.17-config-unknown-field-warnings.md`.

Completed:

- Added `OperonConfig::from_str_with_warnings` and config warning records.
- Split unknown-field scanning into `crates/operon-config/src/warnings.rs`.
- Added unknown field detection for root, daemon, daemon auth, client nodes,
  node auth, policy, secrets, fs mounts, job policy, services, and service
  permissions.
- `OperonConfig::load` now prints warning lines for unknown field paths before
  returning the parsed config.
- Added config and CLI integration tests proving unknown fields warn without
  blocking commands.
- Added `scripts/verify-v0.8.17-config-unknown-field-warnings.sh`.

Remaining:

- No v0.8.17 work remains.
- Future schema additions should update the unknown-field allowlist in
  `operon-config`.

## Phase 61: v0.8.18 Docs, Help, and Skills Synchronization

Status: Completed.

Goal: keep docs, CLI help, repo-local skills, and agent rules synchronized with
the implemented endpoint-only model.

Done when:

- repo-local skills use current endpoint-only discovery commands.
- current docs do not instruct users to run removed provider commands or
  legacy discovery flags.
- validation checks public CLI help paths, skill guidance, AGENTS.md sync
  rules, and stale provider command examples.
- CI runs the synchronization validation.

Detailed plan:
`docs/plan/v0.8.18-docs-help-skills-sync.md`.

Completed:

- Updated repo-local skills and planning docs to use current mDNS discovery
  syntax.
- Added `scripts/verify-docs-help-skills-sync.sh`.
- Added graph/workflow help validation to the docs/help/skills sync gate.
- Added AGENTS.md rules requiring future CLI, config, endpoint, docs, and skill
  changes to keep those surfaces synchronized.
- Added the sync validation to CI and README validation guidance.

Remaining:

- No v0.8.18 work remains.

## Phase 62: v0.9 Endpoint Model Acceptance

Status: Completed.

Goal: make endpoint-only configuration and mDNS discovery reproducible.

Planned:

- `docs/plan/v0.9-acceptance.md`.
- endpoint-only config validation.
- mDNS discovery export validation.
- README updates for configuring endpoints reached through Cloudflare Mesh,
  Tailscale, WireGuard, SSH, LAN, Kubernetes DNS, or manual DNS.

Done when:

- v0.9 has documented acceptance criteria.
- config and discovery validation prove that Operon consumes only endpoints.
- docs explicitly preserve the "Operon is not a VPN" boundary.

Completed:

- Added `scripts/verify-v0.9-endpoint-model.sh`.
- Kept `examples/config.yaml` endpoint-only.
- Added mDNS discovery record coverage for endpoint candidates without provider
  metadata.
- Added discovery export coverage proving generated config contains endpoint
  client nodes and no policy grants.
- Added the v0.9 validation to CI and README validation guidance.

Remaining:

- No v0.9 acceptance work remains.

## Phase 63: Post-v0.9 Discovery UX

Status: Completed.

Goal: improve discovery ergonomics without reintroducing provider abstractions.

Planned:

- clearer conflict handling when mDNS output is exported over an existing
  config.
- optional endpoint health checks during discovery output.
- docs for external scripts that can generate endpoint-only config from
  third-party control planes.

Done when:

- discovery remains endpoint-only.
- generated config never contains provider metadata.
- third-party control-plane examples stay outside the runtime model.

Completed:

- Added `operon node discover --check-health` for best-effort endpoint health
  checks during discovery output.
- Changed `node discover --output-config <path>` to merge new discovered nodes
  into existing config files and reject same-node endpoint conflicts instead of
  silently overwriting.
- Kept discovery export endpoint-only: no provider metadata and no automatic
  policy grants.
- Documented third-party control-plane scripts as external generators of
  endpoint-only config.
- Added `scripts/verify-post-v0.9-discovery-ux.sh` and wired it into CI.

Remaining:

- No post-v0.9 discovery UX work remains.

## Phase 64: v0.9.2 Policy-Derived Capability Discovery

Status: Completed.

Goal: make capability discovery reflect daemon policy instead of a static
default capability set.

Done when:

- filesystem capabilities are derived from configured policy mounts.
- job capability is advertised only when policy permits at least one working
  directory.
- service capabilities are derived from configured services and their
  permissions.
- discovery remains endpoint-only and does not mutate policy.

Completed:

- Added `capabilities_from_policy`.
- Removed the static `default_capabilities` source.
- Updated daemon startup to build `CapabilityList` from `PolicyConfig`.
- Updated service denial audit ids to use `service:<service_id>`.
- Added `scripts/verify-policy-derived-capabilities.sh`.

Remaining:

- No v0.9.2 work remains.

## Phase 65: v0.9.3 Store-Backed Audit Visibility

Status: Completed.

Goal: make daemon audit inspection restart-safe by loading persisted audit
events from the existing append-only JSONL store at startup.

Done when:

- `operon-store` can load `kind: audit` records from the JSONL store.
- daemon startup seeds `AppState.audit` from the configured store.
- startup audit loading keeps the existing bounded in-memory audit retention.
- no protocol, schema, or query database change is introduced.

Completed:

- Added `operon_store::load_audit_events`.
- Updated daemon startup to initialize `AppState.audit` with persisted audit
  events.
- Added `bounded_audit_events` to preserve `MAX_IN_MEMORY_AUDIT_EVENTS` during
  startup reload.
- Added focused store and daemon tests.
- Added `scripts/verify-v0.9.3-store-backed-audit-visibility.sh`.

Remaining:

- No v0.9.3 work remains.

## Phase 66: v0.9.4 Runtime Hardening Consolidation

Status: Completed.

Goal: consolidate the remaining planned runtime hardening candidates into one
bounded phase before adding new product capability surfaces.

Scope:

- service health semantics for TCP/UDP checks and audit reasons.
- restart-safe job log visibility over the existing append-only store, or a
  documented bounded behavior with validation if full reload is not appropriate.
- Linux workspace traversal hardening through `openat2` or fd-relative
  traversal where it materially reduces symlink/race risk.
- protocol-level shell-free `argv[]` job execution if it can be added without
  breaking existing shell-command behavior.
- config UX cleanup for the different LAN advertisement defaults in
  `operon init config` and `operon onboard`.
- focused maintainability cleanup only where the above work touches large
  daemon or CLI files.

Done when:

- TCP and UDP service health behavior is documented, audited, and covered by
  focused tests.
- job log restart behavior is implemented or explicitly bounded with tests and
  docs.
- workspace traversal hardening has Linux-focused tests or a documented
  cross-platform fallback.
- shell-command and argv job execution contracts are clear across protocol,
  CLI, SDK, docs, and tests if argv execution is implemented.
- `init config` and `onboard` explain their LAN advertisement defaults.
- `scripts/verify-v0.9.4-runtime-hardening-consolidation.sh` is added and
  wired into CI.

Detailed plan:
`docs/plan/v0.9.4-runtime-hardening-consolidation.md`.

Completed:

- Added explicit TCP/UDP service health audit reasons, including UDP's
  connection-setup-only semantics.
- Added store-backed job log reload through `operon_store::load_job_logs` and
  bounded daemon `JobLogBuffer` seeding at startup.
- Added explicit workspace traversal hardening strategy coverage and symlink
  parent escape validation.
- Added shell-free job `argv[]` support in proto, Rust core/protocol, daemon,
  CLI, TypeScript SDK, generated TypeScript bindings, and docs.
- Bumped `PROTOCOL_VERSION` to `v0.9.4`.
- Added LAN advertisement default notes for `operon init config` and
  `operon onboard`.
- Added `scripts/verify-v0.9.4-runtime-hardening-consolidation.sh` and wired
  it into CI.

Remaining:

- No v0.9.4 work remains.

## Phase 67: v0.9.5 Policy Language Hardening

Status: Completed.

Goal: make Operon policy easier to reason about, explain, and audit before
adding new capability surfaces.

Scope:

- shared internal policy decision model with stable allow/deny reason codes.
- effective policy explanation in `operon config explain`.
- consistent filesystem, job, service, and secret authorization vocabulary.
- clearer audit denial reasons without breaking existing audit filters.
- additive config/docs/validation changes only; no replacement of the current
  `policy:` schema.

Done when:

- policy decisions carry subject, capability id, action, resource, and stable
  reason code data.
- filesystem, job, service, and secret deny paths use the shared decision
  vocabulary or document why they are separate.
- `operon config explain --json` exposes effective policy grants without
  leaking secrets.
- human `operon config explain` output remains readable and names effective
  grants and limits.
- unknown config field warnings remain non-blocking.
- `scripts/verify-v0.9.5-policy-language-hardening.sh` is added and wired into
  CI.

Detailed plan:
`docs/plan/v0.9.5-policy-language-hardening.md`.

Remaining:

Completed:

- Added `PolicyDecision` and `PolicyReasonCode` to `operon-core`.
- Added shared decision-producing authorization helpers for filesystem, job,
  secret, and service policy checks.
- Updated daemon deny audit paths to record stable policy reason codes with
  human-readable messages.
- Extended `operon config explain` JSON and human output with effective policy
  grants.
- Documented the policy decision vocabulary in README, `PROTOCOL.md`, and
  runtime API docs.
- Added `scripts/verify-v0.9.5-policy-language-hardening.sh` and wired it into
  CI.

Remaining:

- No v0.9.5 work remains.

## Later Candidate Work

No later candidate phases are currently planned.

## Phase 68: v0.9.6 Capability Diagnostics

Status: Completed.

Goal: expose policy decision diagnostics through the runtime, CLI, and SDK so
users and agents can ask why a capability action is allowed or denied before
running it.

Scope:

- daemon-owned gRPC capability diagnostic RPC.
- filesystem, job, secret, and service policy decision explanations.
- `operon capability explain <node> <capability_id> <action> <resource>` with
  `--json` and optional job `--timeout-secs`.
- TypeScript SDK helper for the same diagnostic shape.
- docs, validation script, CI, and agent guidance updates.

Done when:

- diagnostics return subject, capability id, action, resource, allowed,
  reason code, and message.
- daemon diagnostics reuse existing policy decision helpers instead of
  reimplementing policy in the CLI.
- unsupported capability/action pairs return a denied diagnostic with
  `unsupported-action`.
- `scripts/verify-v0.9.6-capability-diagnostics.sh` is added and wired into CI.

Detailed plan:
`docs/plan/v0.9.6-capability-diagnostics.md`.

Remaining:

Completed:

- Added `ExplainCapability` to the active gRPC runtime protocol.
- Added protocol conversions for `CapabilityDiagnosticRequest` and
  `PolicyDecision`, and bumped `PROTOCOL_VERSION` to `v0.9.6`.
- Implemented daemon-owned capability diagnostic dispatch for filesystem, job,
  secret, service, and unsupported action checks.
- Added `operon capability explain` with JSON and human output.
- Added TypeScript SDK `explainCapability` support and regenerated protocol
  bindings.
- Documented capability diagnostics in README, `PROTOCOL.md`, and runtime API
  docs.
- Added `scripts/verify-v0.9.6-capability-diagnostics.sh` and wired it into CI.

Remaining:

- No v0.9.6 work remains.

## Phase 69: v0.9.7 Runtime API Hardening

Status: Completed.

Goal: close review findings around runtime API pagination, streaming API
documentation, SDK memory behavior, and daemon-side job validation.

Scope:

- filesystem list pagination in the active gRPC protocol.
- complete-list compatibility in Rust CLI, Linux mount adapter, and TypeScript
  SDK helpers.
- runtime API documentation alignment for bidirectional service tunnel RPCs.
- TypeScript SDK file writes that do not pre-buffer `ReadableStream` bodies.
- daemon-side rejection of job requests with neither `command` nor `argv`.

Done when:

- `ListFs` accepts `page_size` and `page_token`, and `FsList` exposes
  `next_page_token`.
- protocol conversions preserve fs list pagination metadata.
- daemon fs listing returns deterministic pages and rejects invalid page tokens.
- public CLI/mount/SDK helpers preserve existing full-list behavior by walking
  pages internally.
- TypeScript SDK file writes stream readable bodies into gRPC chunks without
  concatenating the whole body first.
- daemon job startup rejects empty command/argv requests before spawning a
  shell.
- README, `PROTOCOL.md`, runtime API docs, AGENTS.md, and this phase tracker
  are updated.

Detailed plan:
`docs/plan/v0.9.7-runtime-api-hardening.md`.

Remaining:

Completed:

- Added paginated `FsListRequest` and `FsList.next_page_token` to the active
  gRPC runtime protocol, and bumped `PROTOCOL_VERSION` to `v0.9.7`.
- Preserved fs list pagination metadata through Rust core/protocol
  conversions.
- Implemented daemon fs list pagination with deterministic sorted pages and
  invalid-token errors.
- Updated Rust CLI, Linux mount adapter, and TypeScript SDK list helpers to
  walk all pages for their existing public complete-list behavior.
- Updated TypeScript SDK file writes to stream `ReadableStream` bodies lazily
  into gRPC chunks.
- Added daemon validation for empty job requests before spawning a shell.
- Documented the runtime API hardening in README, `PROTOCOL.md`, runtime API
  docs, AGENTS.md, and this tracker.

Remaining:

- No v0.9.7 work remains.

## Planning Principle

Every phase should preserve the core boundary:

```text
Cloudflare Mesh / Tailscale / WireGuard / SSH / LAN solve connectivity.
Operon solves what connected machines are allowed to do, how execution is
composed, and how results are traced.
```
