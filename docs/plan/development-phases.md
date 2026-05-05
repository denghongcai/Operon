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
- Docker two-node validation added through [`docker-compose.yml`](../../docker-compose.yml), [`docker/Dockerfile`](../../docker/Dockerfile), `examples/docker-nodes.yaml`, and [`scripts/verify-mvp-docker.sh`](../../scripts/verify-mvp-docker.sh).
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
- [`operon-network`](../../crates/operon-network): manual endpoint resolver
- [`operon-core`](../../crates/operon-core): shared health and node info types
- [`operon-cli`](../../crates/operon-cli): node commands

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
- [`scripts/verify-mvp-docker.sh`](../../scripts/verify-mvp-docker.sh) now validates both node health and capability discovery.

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
- example Docker workflow added at [`examples/docker-copy-and-run.yaml`](../../examples/docker-copy-and-run.yaml).
- [`examples/train-model.yaml`](../../examples/train-model.yaml) updated with explicit step ids and write content.
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
- README now shows the runnable [`examples/docker-copy-and-run.yaml`](../../examples/docker-copy-and-run.yaml) workflow.
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

- Docker MVP validation script renamed to [`scripts/verify-mvp-docker.sh`](../../scripts/verify-mvp-docker.sh).
- README Quickstart added with the full MVP validation command set.
- README demo command updated to use the MVP validation script.
- `docs/plan/mvp-acceptance.md` added as the v0.1.0 acceptance baseline.
- MVP acceptance document records scope, non-goals, validation commands, release checklist, and known limitations.
- CI now runs on pushes to both `main` and `mvp`.
- CI now includes an `MVP Docker Validation` job that runs [`scripts/verify-mvp-docker.sh`](../../scripts/verify-mvp-docker.sh) after Rust and TypeScript checks.
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
- Renamed Docker validation script to [`scripts/verify-v0.2-docker.sh`](../../scripts/verify-v0.2-docker.sh).
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
- Added [`scripts/verify-v0.5-docker.sh`](../../scripts/verify-v0.5-docker.sh).
- Updated CI to run v0.3 Docker validation.
- Verified [`scripts/verify-v0.5-docker.sh`](../../scripts/verify-v0.5-docker.sh) locally against the two-node Docker environment.
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
- Covered audit filter and trace UX paths in [`scripts/verify-v0.5-docker.sh`](../../scripts/verify-v0.5-docker.sh).

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
- Added [`scripts/verify-v0.5-docker.sh`](../../scripts/verify-v0.5-docker.sh) and made it repeatable around the
  read-only mount PoC temp directory.
- Updated CI to run on pull requests and pushes to every branch.
- Updated CI Docker validation from v0.3 to v0.4.
- Verified [`scripts/verify-v0.5-docker.sh`](../../scripts/verify-v0.5-docker.sh) locally against the two-node Docker
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
- add generated Rust types through [`operon-protocol`](../../crates/operon-protocol).
- document compatibility between migration-era HTTP facade errors and gRPC
  status details.

Done when:

- protobuf schemas cover the current v0.4 runtime capabilities.
- [`operon-protocol`](../../crates/operon-protocol) builds generated Rust bindings.
- protocol docs describe which methods are unary, server-streaming,
  client-streaming, or bidirectional.

Completed:

- Added [`proto/operon/runtime.proto`](../../proto/operon/runtime.proto) as the v0.5 runtime contract.
- Generated Rust bindings from [`operon-protocol`](../../crates/operon-protocol) with tonic/prost.
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

- Added [`scripts/verify-v0.5-docker.sh`](../../scripts/verify-v0.5-docker.sh) as the canonical two-node gRPC
  validation.
- Added `examples/docker-nodes.yaml`.
- Updated CI with v0.5 Docker validation and `protoc` installation for Rust
  protocol generation.
- Verified locally with `cargo test --workspace`, `pnpm typecheck`,
  `pnpm test`, and [`scripts/verify-v0.5-docker.sh`](../../scripts/verify-v0.5-docker.sh).

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
- remove the hand-written HTTP client path from [`operon-cli`](../../crates/operon-cli).
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
- [`operon-cli`](../../crates/operon-cli) runtime commands use the gRPC client path only.
- TypeScript SDK calls `nice-grpc` directly and no longer has fetch/HTTP
  fallback behavior.
- Docker, CI, and example node configs use `grpc://` endpoints.
- Root `PROTOCOL.md` documents direct protocol integration without an SDK.
- Validation passed with `cargo fmt --check`, `cargo check --workspace
  --locked`, `cargo test --workspace --locked`, `cargo clippy --workspace
  --locked -- -D warnings`, `pnpm typecheck`, `pnpm -r test`,
  `pnpm --filter @operon/sdk build`, [`scripts/verify-v0.5-docker.sh`](../../scripts/verify-v0.5-docker.sh), and
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

Goal: implement a real Linux mount path in [`operon-mount`](../../crates/operon-mount).

Planned:

- add Linux-only FUSE dependencies.
- implement lookup, getattr, readdir, open, read, and release.
- keep write operations out of v0.6.
- route all remote fs operations through existing policy-enforced daemon APIs.
- record audit events through the remote node for mounted operations.

Completed:

- Added `fuser` and implemented a read-only FUSE adapter in [`operon-mount`](../../crates/operon-mount).
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
- Added [`scripts/verify-v0.6-linux-mount.sh`](../../scripts/verify-v0.6-linux-mount.sh) with host requirement checks.
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
  `pnpm typecheck`, `pnpm -r test`, [`scripts/verify-v0.5-docker.sh`](../../scripts/verify-v0.5-docker.sh),
  [`scripts/verify-v0.6-linux-mount.sh`](../../scripts/verify-v0.6-linux-mount.sh), and `git diff --check`.

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

- Extended [`proto/operon/runtime.proto`](../../proto/operon/runtime.proto) with write-range, truncate, mkdir,
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
- Added [`scripts/verify-v0.6.1-linux-write-mount.sh`](../../scripts/verify-v0.6.1-linux-write-mount.sh).
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
- Added [`scripts/verify-v0.6.2-cli-fs-cleanup.sh`](../../scripts/verify-v0.6.2-cli-fs-cleanup.sh).
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
  [`proto/operon/runtime.proto`](../../proto/operon/runtime.proto).
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
- Added [`scripts/verify-v0.6.3-fs-copy.sh`](../../scripts/verify-v0.6.3-fs-copy.sh).
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
- Added [`scripts/verify-v0.6.4-onboard.sh`](../../scripts/verify-v0.6.4-onboard.sh).
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

Goal: put configuration ownership in a dedicated crate instead of [`operon-network`](../../crates/operon-network).

Planned:

- add [`operon-config`](../../crates/operon-config).
- define `OperonConfig`, daemon config, client config, node config, auth config,
  and secret references.
- keep provider values available for client node resolution.
- resolve relative file references from the config file directory.

Completed:

- Added [`operon-config`](../../crates/operon-config) as the shared schema/loading crate.
- Moved unified config, node endpoint, provider, auth, daemon, client, and
  secret reference types into [`operon-config`](../../crates/operon-config).
- Kept [`operon-network`](../../crates/operon-network) as a thin re-export boundary for provider/node endpoint
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
- LAN mDNS discovery is centralized in [`operon-network`](../../crates/operon-network) and reused by `node
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

- moved workspace path containment and fs policy helpers into [`operon-fs`](../../crates/operon-fs).
- moved job authorization and environment construction into [`operon-process`](../../crates/operon-process).
- moved append-only store helpers into [`operon-store`](../../crates/operon-store).
- moved service health check helper into [`operon-network`](../../crates/operon-network).
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
- added [`scripts/verify-v0.6.7-runtime.sh`](../../scripts/verify-v0.6.7-runtime.sh) to validate descendant termination
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

- convert [`operon-cli`](../../crates/operon-cli) entrypoint to an explicit Tokio runtime, preferably
  `#[tokio::main] async fn main()`.
- convert [`crates/operon-cli/src/grpc.rs`](../../crates/operon-cli/src/grpc.rs) public gRPC helper functions to
  async functions.
- remove `OnceLock<tokio::runtime::Runtime>` and the internal `block_on`
  wrapper from [`grpc.rs`](../../crates/operon-cli/src/grpc.rs).
- propagate `.await` through CLI command handlers and graph execution where
  they call gRPC.
- preserve synchronous local file/config parsing where there is no runtime
  benefit to changing it.
- keep `operon_mount::spawn_mount` unchanged unless the mount command requires a
  follow-up integration adjustment.

Completed:

- CLI entrypoint now owns the Tokio runtime explicitly.
- [`operon-cli/src/grpc.rs`](../../crates/operon-cli/src/grpc.rs) no longer owns a singleton runtime or internal
  `block_on` wrapper.
- gRPC helper functions are async and command handlers/graph execution await
  them directly.
- request context propagation moved to a Tokio task-local so graph audit
  metadata survives async execution.

Done when:

- [`operon-cli/src/grpc.rs`](../../crates/operon-cli/src/grpc.rs) no longer creates or owns a Tokio runtime.
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

- created and executed [`scripts/verify-v0.6.7-runtime.sh`](../../scripts/verify-v0.6.7-runtime.sh).
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
- [`crates/operon-protocol/build.rs`](../../crates/operon-protocol/build.rs) remains focused on the active runtime API.

Completed:

- Moved inactive proto files to [`proto/archive/operon/`](../../proto/archive/operon).
- Kept [`crates/operon-protocol/build.rs`](../../crates/operon-protocol/build.rs) focused on
  [`proto/operon/runtime.proto`](../../proto/operon/runtime.proto).
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
- Expanded [`scripts/verify-v0.6.7-runtime.sh`](../../scripts/verify-v0.6.7-runtime.sh) and added it to CI.
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
- CI runs [`scripts/verify-v0.6.9-cli-contract.sh`](../../scripts/verify-v0.6.9-cli-contract.sh).

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
- Added unit tests and [`scripts/verify-v0.6.9-cli-contract.sh`](../../scripts/verify-v0.6.9-cli-contract.sh), and wired the
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
- Added focused tests plus [`scripts/verify-v0.6.10-runtime-hardening.sh`](../../scripts/verify-v0.6.10-runtime-hardening.sh) to CI.

Remaining:

- No open v0.6.10 items.

## Phase 32.32: v0.6.11 Maintainability Governance

Status: Completed.

Goal: reduce the highest-risk maintenance issues before starting larger feature
work.

Planned:

- `docs/plan/v0.6.11-maintainability-governance.md`.
- split daemon defaults, LAN advertise, store-path validation, status mapping,
  and lock handling out of [`operond/src/main.rs`](../../crates/operond/src/main.rs).
- make gRPC-facing daemon lock acquisition return `Status::internal` instead of
  panicking on poisoned mutexes.
- make Linux-only mount support explicit through target-specific dependencies
  and a non-Linux CLI error path.
- add focused validation coverage for the governance checks.

Done when:

- the high-risk daemon helper areas have module boundaries.
- gRPC request paths no longer use direct poisoned-lock `expect` handling for
  shared runtime state.
- non-Linux builds are not forced to compile [`operon-mount`](../../crates/operon-mount).
- CI runs [`scripts/verify-v0.6.11-governance.sh`](../../scripts/verify-v0.6.11-governance.sh).
- workspace validation passes.

Completed:

- Added `docs/plan/v0.6.11-maintainability-governance.md`.
- Split `operond` support code into `defaults`, `grpc_status`,
  `lan_advertise`, `locks`, and `store_config` modules.
- Removed direct poisoned-lock `expect` calls from [`operond/src/main.rs`](../../crates/operond/src/main.rs).
- Added a gRPC lock helper that maps poisoned shared-state locks to
  `Status::internal`.
- Changed background job/audit cleanup paths to log poisoned locks and return
  instead of panicking.
- Made [`operon-cli`](../../crates/operon-cli) depend on [`operon-mount`](../../crates/operon-mount) only on Linux targets.
- Added a non-Linux `operon mount` unsupported-platform error path.
- Added [`scripts/verify-v0.6.11-governance.sh`](../../scripts/verify-v0.6.11-governance.sh) and wired it into CI.

Remaining:

- Larger domain splits remain future work: `operond` server/fs/job/audit
  modules, [`operon-cli`](../../crates/operon-cli) command modules, and [`operon-mount`](../../crates/operon-mount) remote/inode/FUSE
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
- promote [`operon-store`](../../crates/operon-store) to an explicit append-only event writer boundary with
  visible fsync policy and `Result`-returning append operations.
- surface store append failures at daemon runtime boundaries.
- consolidate daemon background job/log/audit lock handling through runtime
  helper boundaries instead of scattered `eprintln!` paths.
- make [`operon-mount`](../../crates/operon-mount) a Linux FUSE adapter boundary by excluding the `fuser`
  dependency outside Linux.
- add focused validation coverage and wire it into CI.

Done when:

- `StreamJobLogs` returns envelope messages.
- CLI JSON and stream output preserve job-log truncation metadata.
- TS SDK exposes real stream events for job logs.
- [`operon-store`](../../crates/operon-store) append failures are testable and no longer swallowed inside the
  store crate.
- daemon persistence failures are logged consistently at the daemon boundary.
- non-Linux builds do not resolve `fuser` through [`operon-mount`](../../crates/operon-mount).
- CI runs [`scripts/verify-v0.6.12-runtime-boundary.sh`](../../scripts/verify-v0.6.12-runtime-boundary.sh).
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
- Added `StoreWriter` and `FsyncPolicy` to [`operon-store`](../../crates/operon-store); append failures now
  return `Result`.
- Routed daemon append-only persistence through the store writer boundary and
  logged persistence failures at daemon runtime boundaries.
- Replaced remaining background mutex-poison `eprintln!` paths in daemon
  runtime helpers with structured tracing errors.
- Made [`operon-mount`](../../crates/operon-mount) a Linux-only FUSE adapter boundary by gating the crate and
  the `fuser` dependency to Linux.
- Updated protocol docs, runtime architecture docs, README release examples,
  and the public protocol version to v0.6.12.
- Completed a post-release documentation drift pass that aligned current docs
  with v0.6.12 and marked older acceptance docs as historical snapshots.
- Added [`scripts/verify-v0.6.12-runtime-boundary.sh`](../../scripts/verify-v0.6.12-runtime-boundary.sh) and wired it into CI.
- Validation passed:
  - [`scripts/verify-v0.6.12-runtime-boundary.sh`](../../scripts/verify-v0.6.12-runtime-boundary.sh)
  - [`scripts/verify-v0.6.7-runtime.sh`](../../scripts/verify-v0.6.7-runtime.sh)
  - [`scripts/verify-v0.6.9-cli-contract.sh`](../../scripts/verify-v0.6.9-cli-contract.sh)
  - [`scripts/verify-v0.6.10-runtime-hardening.sh`](../../scripts/verify-v0.6.10-runtime-hardening.sh)
  - [`scripts/verify-v0.6.11-governance.sh`](../../scripts/verify-v0.6.11-governance.sh)
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
- added [`scripts/verify-v0.7-service-forwarding.sh`](../../scripts/verify-v0.7-service-forwarding.sh) with a local HTTP service,
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

- added [`scripts/verify-v0.7.1-udp-datagram-forwarding.sh`](../../scripts/verify-v0.7.1-udp-datagram-forwarding.sh).
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
- added [`scripts/verify-v0.8.1-integration-coverage.sh`](../../scripts/verify-v0.8.1-integration-coverage.sh), which starts a real
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

- changed audit timestamps to use `u64` end-to-end across [`operon-core`](../../crates/operon-core) and
  [`operon-protocol`](../../crates/operon-protocol), matching the gRPC `uint64` schema.
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
- document that GitHub release tags, Rust crate versions, TS SDK package
  versions, and `PROTOCOL_VERSION` must align for public releases.

Done when:

- FUSE random reads use `ReadFileRange`.
- daemon range-read validation prevents offset/size overflow.
- protocol and SDK tests cover the new API surface.
- README and release docs do not imply `v0.6.12` is the current install target.
- version policy explains that public release preparation must align release,
  package, and protocol versions together.

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
  [`scripts/verify-v0.8.3-read-range-release-cleanup.sh`](../../scripts/verify-v0.8.3-read-range-release-cleanup.sh).

## Phase 47: v0.8.4 Runtime and CLI Modularization

Status: Completed.

Goal: reduce the largest maintenance hotspots through behavior-preserving
module splits before adding endpoint discovery UX.

Plan:

- split [`crates/operond/src/main.rs`](../../crates/operond/src/main.rs) so it keeps startup wiring and top-level
  command dispatch, while fs, job, service forwarding, audit, pagination, and
  runtime state move into focused modules.
- split [`crates/operon-cli/src/main.rs`](../../crates/operon-cli/src/main.rs) so it keeps clap model construction and
  high-level dispatch, while command families, output rendering, and target
  parsing move into focused modules.
- preserve current public CLI behavior, gRPC behavior, JSON output, quiet
  output, and failure exit semantics.
- add focused module-level tests where extraction exposes pure helpers.

Done when:

- [`operond/src/main.rs`](../../crates/operond/src/main.rs) no longer directly owns fs, job, service-forwarding,
  audit, and pagination implementation details.
- [`operon-cli/src/main.rs`](../../crates/operon-cli/src/main.rs) no longer directly owns every command handler and
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
- Validation passed with [`scripts/verify-v0.8.4-modularization.sh`](../../scripts/verify-v0.8.4-modularization.sh).

Remaining:

- Job runtime, service forwarding, audit helpers, and non-fs CLI command
  families still need follow-up extraction before major feature work in those
  areas.

## Phase 48: v0.8.5 Core Domain Module Boundaries

Status: Completed.

Goal: split [`operon-core`](../../crates/operon-core) into domain modules before endpoint discovery UX and
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

- [`crates/operon-core/src/lib.rs`](../../crates/operon-core/src/lib.rs) only wires modules, re-exports public types,
  and keeps crate-level tests.
- serialized YAML/JSON names, gRPC schemas, SDK APIs, CLI behavior, and daemon
  behavior do not change.
- full Rust validation remains green.

Detailed plan: `docs/plan/v0.8.5-core-domain-module-boundaries.md`.

Completed:

- Split [`operon-core`](../../crates/operon-core) into `runtime`, `fs`, `job`, `service`, `policy`,
  `audit`, `discovery`, and `trace` modules.
- Kept root-level public re-exports so current downstream imports continue to
  work.
- Preserved serde formats, gRPC schemas, SDK APIs, CLI behavior, and daemon
  behavior.
- Added module path / root re-export coverage in [`operon-core`](../../crates/operon-core) tests.
- Added [`scripts/verify-v0.8.5-core-domain-modules.sh`](../../scripts/verify-v0.8.5-core-domain-modules.sh) and wired it into CI.

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
- split non-fs [`operon-cli`](../../crates/operon-cli) command families into `commands/*` modules and
  reduce repeated text/json/quiet rendering branches where practical.
- add a lightweight Rust [`operon-grpc-client`](../../crates/operon-grpc-client) crate for tonic endpoint URI
  normalization, auth/context metadata, typed client construction, and Rust-side
  stream chunk helpers shared by CLI and mount.
- split [`operon-mount`](../../crates/operon-mount) into remote client, inode table, FUSE callbacks, path,
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

- [`crates/operond/src/main.rs`](../../crates/operond/src/main.rs) no longer directly owns job runtime, job log
  retention, audit append, TCP service tunnel, or UDP datagram tunnel internals.
- [`crates/operon-cli/src/main.rs`](../../crates/operon-cli/src/main.rs) no longer owns non-fs command handlers and
  renderers.
- CLI and mount share Rust gRPC endpoint/auth/client helpers.
- [`operon-mount`](../../crates/operon-mount) has module boundaries for remote client, inode table, FUSE
  callbacks, paths, errors, and session lifecycle.
- TypeScript SDK exposes direct public methods for the listed core protocol
  capabilities.
- behavior-sensitive CLI/SDK/script contracts remain green.

Detailed plan:
`docs/plan/v0.8.6-runtime-cli-client-modularization.md`.

Completed:

- Added [`operon-grpc-client`](../../crates/operon-grpc-client) and migrated CLI plus Linux mount gRPC callers to
  shared endpoint/auth/context/client/chunk helpers.
- Split non-fs CLI command handlers into `commands/*` modules and reduced
  [`operon-cli/src/main.rs`](../../crates/operon-cli/src/main.rs) to Clap model construction and high-level dispatch.
- Added `operon graph run` and `operon workflow run` aliases while preserving
  top-level `operon run`.
- Updated `operon --json fs read <target> --output <file>` to emit a
  structured `{ path, output, bytes_written }` summary.
- Split Linux mount internals into remote client, inode table, FUSE callbacks,
  path, errors, and session modules.
- Split daemon auth, audit, state, job runtime/log retention, and service
  forwarding internals out of [`operond/src/main.rs`](../../crates/operond/src/main.rs).
- Exposed direct TypeScript SDK methods for capabilities, fs stat/list, job
  run/get/cancel, and audit listing.
- Added reusable validation helpers in [`scripts/lib/validation.sh`](../../scripts/lib/validation.sh).
- Added [`scripts/verify-v0.8.6-runtime-cli-client-modularization.sh`](../../scripts/verify-v0.8.6-runtime-cli-client-modularization.sh) and wired
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

- [`crates/operond/src/fs_service.rs`](../../crates/operond/src/fs_service.rs) repeated the same `authorize_fs`, path
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
  [`crates/operond/src/fs_service.rs`](../../crates/operond/src/fs_service.rs).
- Reused those helpers across stat, list, read range, write range, truncate,
  mkdir, delete, rename, and copy operations.
- Added [`scripts/verify-v0.8.7-fs-service-reuse-cleanup.sh`](../../scripts/verify-v0.8.7-fs-service-reuse-cleanup.sh).

Remaining:

- No v0.8.7 work remains.
- Moving the full tonic `GrpcRuntime` trait implementation out of
  [`operond/src/main.rs`](../../crates/operond/src/main.rs) remains a future maintainability candidate if runtime
  method routing grows again.

## Phase 51: v0.8.8 Filesystem Stream Handler Cleanup

Status: Completed.

Goal: keep full-file filesystem stream behavior inside the daemon filesystem
service module instead of the tonic runtime router.

Review finding:

- [`crates/operond/src/main.rs`](../../crates/operond/src/main.rs) still owned full-file `ReadFile` and
  `WriteFile` authorization, workspace path resolution, audit failure handling,
  chunk-size validation, and file IO.
- That duplicated the filesystem service boundary improved in v0.8.7 and kept
  filesystem business logic in the gRPC router.

Done when:

- [`fs_service.rs`](../../crates/operond/src/fs_service.rs) owns full-file read and write stream handlers.
- [`operond/src/main.rs`](../../crates/operond/src/main.rs) only performs gRPC auth, audit context scoping, and
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
- Added [`scripts/verify-v0.8.8-fs-stream-handler-cleanup.sh`](../../scripts/verify-v0.8.8-fs-stream-handler-cleanup.sh).

Remaining:

- No v0.8.8 work remains.
- Moving the full tonic `GrpcRuntime` trait implementation out of
  [`operond/src/main.rs`](../../crates/operond/src/main.rs) remains a future maintainability candidate if runtime
  method routing grows again.

## Phase 52: v0.8.9 Service Tunnel Boundary Cleanup

Status: Completed.

Goal: keep service tunnel target parsing, authorization, protocol checks, audit
handling, and connection setup inside the daemon service forwarding module.

Review finding:

- [`crates/operond/src/main.rs`](../../crates/operond/src/main.rs) still owned TCP and UDP service tunnel open
  handshakes: target-envelope validation, service policy authorization,
  protocol mismatch checks, audit records, TCP connection setup, and datagram
  stream delegation.
- That kept service forwarding business logic in the gRPC router instead of
  behind [`service_forward.rs`](../../crates/operond/src/service_forward.rs).

Done when:

- [`service_forward.rs`](../../crates/operond/src/service_forward.rs) owns TCP and UDP tunnel open/handshake logic.
- [`operond/src/main.rs`](../../crates/operond/src/main.rs) only performs gRPC auth, audit context scoping, and
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
- Added [`scripts/verify-v0.8.9-service-tunnel-boundary-cleanup.sh`](../../scripts/verify-v0.8.9-service-tunnel-boundary-cleanup.sh).

Remaining:

- No v0.8.9 work remains.
- Moving the full tonic `GrpcRuntime` trait implementation out of
  [`operond/src/main.rs`](../../crates/operond/src/main.rs) remains a future maintainability candidate if runtime
  method routing grows again.

## Phase 53: v0.8.10 Mount Lock Hardening

Status: Completed.

Goal: make Linux FUSE mount callbacks return filesystem errors instead of
panicking when the inode table lock is poisoned.

Review finding:

- [`crates/operon-mount/src/fuse_fs.rs`](../../crates/operon-mount/src/fuse_fs.rs) used repeated
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

- Added `write_inodes` in [`crates/operon-mount/src/fuse_fs.rs`](../../crates/operon-mount/src/fuse_fs.rs).
- Replaced direct write-lock `expect` calls across lookup/upsert, setattr,
  unlink, rmdir, rename, write cache refresh, and readdir paths.
- Added [`scripts/verify-v0.8.10-mount-lock-hardening.sh`](../../scripts/verify-v0.8.10-mount-lock-hardening.sh).

Remaining:

- No v0.8.10 work remains.
- Broader Linux mount callback decomposition remains a future candidate if the
  FUSE adapter grows beyond a thin adapter boundary.

## Phase 54: v0.8.11 CLI Datagram Lock Hardening

Status: Completed.

Goal: make CLI UDP/datagram forwarding report peer-state lock failures instead
of panicking.

Review finding:

- [`crates/operon-cli/src/grpc.rs`](../../crates/operon-cli/src/grpc.rs) used
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
- Added [`scripts/verify-v0.8.11-cli-datagram-lock-hardening.sh`](../../scripts/verify-v0.8.11-cli-datagram-lock-hardening.sh).

Remaining:

- No v0.8.11 work remains.
- Broader [`operon-cli/src/grpc.rs`](../../crates/operon-cli/src/grpc.rs) command-family split remains a future
  maintainability candidate.

## Phase 55: v0.8.12 Daemon Datagram Invariant Cleanup

Status: Completed.

Goal: remove the remaining production invariant panic from daemon UDP/datagram
forwarding.

Review finding:

- [`crates/operond/src/service_forward.rs`](../../crates/operond/src/service_forward.rs) used
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
- Added [`scripts/verify-v0.8.12-daemon-datagram-invariant-cleanup.sh`](../../scripts/verify-v0.8.12-daemon-datagram-invariant-cleanup.sh).

Remaining:

- No v0.8.12 work remains.
- Broader service datagram state-machine extraction remains a future candidate
  if UDP forwarding behavior grows.

## Phase 56: v0.8.13 Production Panic Cleanup

Status: Completed.

Goal: remove the production panic-style invariants found in daemon job-log
handling and Linux mount remote client runtime access.

Review finding:

- [`crates/operond/src/job_runtime.rs`](../../crates/operond/src/job_runtime.rs) used
  `expect("just pushed job log")` after appending a job log entry.
- [`crates/operon-mount/src/remote_client.rs`](../../crates/operon-mount/src/remote_client.rs) used
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
- Added [`scripts/verify-v0.8.13-production-panic-cleanup.sh`](../../scripts/verify-v0.8.13-production-panic-cleanup.sh).

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

- [`crates/operon-cli/src/onboard.rs`](../../crates/operon-cli/src/onboard.rs) used
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
- Added [`scripts/verify-v0.8.14-onboard-invariant-cleanup.sh`](../../scripts/verify-v0.8.14-onboard-invariant-cleanup.sh).

Remaining:

- No v0.8.14 work remains.
- [`operon-cli`](../../crates/operon-cli) still contains test-only assertion panics and one
  `String` formatting invariant in token generation; those do not represent
  user-triggered onboarding panics.

## Phase 58: v0.8.15 Token Generation Panic Cleanup

Status: Completed.

Goal: remove the remaining production panic-style token formatting invariant
from CLI private-file helpers.

Review finding:

- [`crates/operon-cli/src/private_files.rs`](../../crates/operon-cli/src/private_files.rs) formatted generated token bytes
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
- Added [`scripts/verify-v0.8.15-token-generation-panic-cleanup.sh`](../../scripts/verify-v0.8.15-token-generation-panic-cleanup.sh).

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
- Added [`scripts/verify-v0.8.16-endpoint-model-simplification.sh`](../../scripts/verify-v0.8.16-endpoint-model-simplification.sh).

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
- Split unknown-field scanning into [`crates/operon-config/src/warnings.rs`](../../crates/operon-config/src/warnings.rs).
- Added unknown field detection for root, daemon, daemon auth, client nodes,
  node auth, policy, secrets, fs mounts, job policy, services, and service
  permissions.
- `OperonConfig::load` now prints warning lines for unknown field paths before
  returning the parsed config.
- Added config and CLI integration tests proving unknown fields warn without
  blocking commands.
- Added [`scripts/verify-v0.8.17-config-unknown-field-warnings.sh`](../../scripts/verify-v0.8.17-config-unknown-field-warnings.sh).

Remaining:

- No v0.8.17 work remains.
- Future schema additions should update the unknown-field allowlist in
  [`operon-config`](../../crates/operon-config).

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
- Added [`scripts/verify-docs-help-skills-sync.sh`](../../scripts/verify-docs-help-skills-sync.sh).
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

- Added [`scripts/verify-v0.9-endpoint-model.sh`](../../scripts/verify-v0.9-endpoint-model.sh).
- Kept [`examples/config.yaml`](../../examples/config.yaml) endpoint-only.
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
- Added [`scripts/verify-post-v0.9-discovery-ux.sh`](../../scripts/verify-post-v0.9-discovery-ux.sh) and wired it into CI.

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
- Added [`scripts/verify-policy-derived-capabilities.sh`](../../scripts/verify-policy-derived-capabilities.sh).

Remaining:

- No v0.9.2 work remains.

## Phase 65: v0.9.3 Store-Backed Audit Visibility

Status: Completed.

Goal: make daemon audit inspection restart-safe by loading persisted audit
events from the existing append-only JSONL store at startup.

Done when:

- [`operon-store`](../../crates/operon-store) can load `kind: audit` records from the JSONL store.
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
- Added [`scripts/verify-v0.9.3-store-backed-audit-visibility.sh`](../../scripts/verify-v0.9.3-store-backed-audit-visibility.sh).

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
- [`scripts/verify-v0.9.4-runtime-hardening-consolidation.sh`](../../scripts/verify-v0.9.4-runtime-hardening-consolidation.sh) is added and
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
- Added [`scripts/verify-v0.9.4-runtime-hardening-consolidation.sh`](../../scripts/verify-v0.9.4-runtime-hardening-consolidation.sh) and wired
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
- [`scripts/verify-v0.9.5-policy-language-hardening.sh`](../../scripts/verify-v0.9.5-policy-language-hardening.sh) is added and wired into
  CI.

Detailed plan:
`docs/plan/v0.9.5-policy-language-hardening.md`.

Remaining:

Completed:

- Added `PolicyDecision` and `PolicyReasonCode` to [`operon-core`](../../crates/operon-core).
- Added shared decision-producing authorization helpers for filesystem, job,
  secret, and service policy checks.
- Updated daemon deny audit paths to record stable policy reason codes with
  human-readable messages.
- Extended `operon config explain` JSON and human output with effective policy
  grants.
- Documented the policy decision vocabulary in README, `PROTOCOL.md`, and
  runtime API docs.
- Added [`scripts/verify-v0.9.5-policy-language-hardening.sh`](../../scripts/verify-v0.9.5-policy-language-hardening.sh) and wired it into
  CI.

Remaining:

- No v0.9.5 work remains.

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
- [`scripts/verify-v0.9.6-capability-diagnostics.sh`](../../scripts/verify-v0.9.6-capability-diagnostics.sh) is added and wired into CI.

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
- Added [`scripts/verify-v0.9.6-capability-diagnostics.sh`](../../scripts/verify-v0.9.6-capability-diagnostics.sh) and wired it into CI.

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
- Follow-up validation maintenance aligned older CI validation scripts with the
  v0.9.7 protocol version and the README/DEVELOPMENT documentation split.
- Follow-up README validation ran the public release Quickstart in Docker,
  aligned user-facing examples with the real onboard defaults, documented
  skills prerequisites, and added [`scripts/verify-readme-quickstart-docker.sh`](../../scripts/verify-readme-quickstart-docker.sh).
- Follow-up release hardening moved Rust release builds into an Ubuntu 20.04
  container, pinned a modern `protoc` for proto3 optional support, documented
  the glibc 2.31+ Linux binary baseline, and added
  [`scripts/verify-release-glibc-baseline.sh`](../../scripts/verify-release-glibc-baseline.sh).
- Follow-up README validation tightened the agent skills prerequisite to
  Node.js 18+ and made the Ubuntu 20.04 Docker quickstart validation install
  Node.js 20 before running the Vercel Skills CLI.
- Follow-up version alignment bumped `PROTOCOL_VERSION`, Rust crate versions,
  and the TypeScript SDK package version to `v0.9.9` / `0.9.9`, exposed
  `operon --version` and `operond --version`, and updated the release policy so
  future public releases keep tag, package, and runtime health versions aligned.
- Follow-up documentation link audit linked references to source files, crates,
  protocol files, scripts, workflows, examples, and skills across README,
  DEVELOPMENT, architecture docs, phase docs, quality docs, AGENTS.md, and
  repo-local skills.

Remaining:

- No v0.9.7 work remains.

## Phase 70: v0.10 Execution Capability Unification

Status: Completed.

Purpose: replace the historical `job` concept with a unified `exec` capability
that covers non-interactive command execution and leaves a clear future slot
for PTY-backed interactive sessions.

Scope:

- remove user-facing `job` terminology from active CLI, SDK, protocol, policy,
  capability, audit, graph, docs, examples, validation scripts, and repo-local
  skills.
- replace it with `exec.run` for non-interactive execution records, logs,
  status, cancellation, timeout, stdin piping, environment policy, and audit.
- document `exec.session` as the future PTY/TTY execution mode without adding
  an unimplemented RPC in v0.10.
- avoid a conservative compatibility alias: the active pre-1.0 surface moves
  directly to `exec` instead of preserving `operon job` as a supported command.
- keep shell-string execution and shell-free `argv[]` execution as modes under
  `exec.run`, with agents and SDK clients preferring `argv[]`.
- update execution graph action names from `job.run` to `exec.run`.
- bump Rust crate versions, the TypeScript SDK package version, and
  `PROTOCOL_VERSION` to `v0.10.0` / `0.10.0`.

Detailed plan:
`docs/plan/v0.10-exec-unification.md`.

Completed:

- Replaced active gRPC runtime execution RPCs, messages, enums, and capability
  kind with `Exec*` / `CAPABILITY_KIND_EXEC`.
- Replaced daemon runtime, core DTOs, process helpers, store persistence
  helpers, CLI command modules, graph actions, policy, audit, examples, and
  TypeScript SDK helpers with exec vocabulary.
- Removed `operon job` from the active CLI instead of keeping a compatibility
  alias.
- Updated README, `PROTOCOL.md`, runtime API docs, DEVELOPMENT.md, AGENTS.md,
  repo-local skills, examples, validation scripts, and CI validation.
- Added [`scripts/verify-v0.10-exec-unification.sh`](../../scripts/verify-v0.10-exec-unification.sh)
  to validate the active exec surface and stale job command removal.
- Follow-up release governance update added an AGENTS.md rule that public
  release tags must be created only after the release commit is merged to
  `main`, and from the commit intended for `main`.

Remaining:

- Nothing remains in v0.10.

## Phase 71: v0.10.1 Filesystem Consistency and Workspace Hardening

Status: Completed.

Purpose: define a concrete filesystem consistency contract and tighten Linux
workspace containment before adding more high-level filesystem behavior.

Scope:

- add opaque filesystem versions to stat/list/write/copy responses.
- add optional filesystem mutation preconditions so clients can send
  `expected_version` or `require_absent` guards and receive gRPC
  `FAILED_PRECONDITION` on stale writes.
- expose guarded writes through the CLI and TypeScript SDK.
- add Linux fd-relative workspace containment validation with
  `openat2(RESOLVE_BENEATH)` where the kernel supports it, while keeping the
  canonical fallback for unsupported kernels and non-Linux hosts.
- document the updated consistency contract in `PROTOCOL.md` and runtime API
  docs.
- add validation coverage in
  [`scripts/verify-v0.10.1-fs-consistency-workspace-hardening.sh`](../../scripts/verify-v0.10.1-fs-consistency-workspace-hardening.sh).

Detailed plan:
`docs/plan/v0.10.1-fs-consistency-workspace-hardening.md`.

Completed:

- Added `FsPrecondition` and filesystem version fields to the runtime protocol.
- Daemon filesystem mutations now validate version/absence preconditions and
  return `FAILED_PRECONDITION` on stale writes.
- Linux workspace path checks now attempt fd-relative
  `openat2(RESOLVE_BENEATH)` validation in addition to canonical containment.
- CLI `fs write` and TypeScript SDK `writeFileBytes` can send guarded writes.
- Protocol/runtime docs and validation scripts are aligned.

Remaining:

- No v0.10.1 work remains.

## Phase 72: v0.10.2 Operator Diagnostics

Status: Completed.

Purpose: provide one operator-facing diagnostic entrypoint that explains common
setup and runtime problems without requiring users or agents to stitch together
multiple commands manually.

Scope:

- add `operon doctor`.
- report config unknown-field warnings, endpoint health, auth/token failures,
  protocol version mismatches, capability diagnostics, and service health.
- support human output and `--json` for scripts and agents.
- reuse daemon-owned policy diagnostics from `ExplainCapability` instead of
  duplicating authorization logic in the CLI.
- document when to use doctor output versus lower-level commands.
- add validation coverage in
  [`scripts/verify-v0.10.2-operator-diagnostics.sh`](../../scripts/verify-v0.10.2-operator-diagnostics.sh).

Detailed plan:
`docs/plan/v0.10.2-operator-diagnostics.md`.

Completed:

- Added top-level `operon doctor` with human and JSON output.
- Doctor reports config unknown fields, endpoint/auth resolution errors,
  health/protocol status, daemon-owned capability diagnostics, and service
  health checks.
- README, DEVELOPMENT, AGENTS.md, and repo-local CLI guidance point users and
  agents to `operon doctor` for first-pass troubleshooting.
- Follow-up version alignment bumped Rust crate versions, the TypeScript SDK
  package version, and `PROTOCOL_VERSION` to `v0.10.2` / `0.10.2`.

Remaining:

- No v0.10.2 work remains.

## Phase 73: v0.11 Exec Session / PTY Interactive

Status: Completed.

Goal: add a true interactive execution surface for terminal-like workflows
after the v0.10 execution vocabulary migration removed the historical `job`
concept.

Detailed plan: `docs/plan/v0.11-exec-session-pty-interactive.md`.

Completed:

- Added `OpenExecSession` as a bidirectional streaming protocol with explicit
  start, input, resize, started, output, and exit envelopes.
- Added `policy.exec.allow_sessions` and `exec:default` `session`
  authorization distinct from `exec.run`.
- Added PTY-backed daemon session execution through a dedicated
  [`exec_session.rs`](../../crates/operond/src/exec_session.rs) module.
- Added `operon exec session` and TypeScript SDK `openExecSession`.
- Bumped Rust crate versions, TypeScript SDK package version, and
  `PROTOCOL_VERSION` to `v0.11.0` / `0.11.0`.
- Updated README, `PROTOCOL.md`, runtime API docs, repo-local skills,
  DEVELOPMENT.md, CI, and validation coverage.

Remaining:

- No v0.11 work remains.
- macOS and Windows PTY validation remains future distribution/platform work.

## Phase 74: v0.10.4 Maintainability Cleanup

Status: Completed.

Goal: continue behavior-preserving modularization around the remaining large
runtime and CLI files so future feature work stays cheap and localized.

Detailed plan: `docs/plan/v0.10.4-maintainability-cleanup.md`.

Completed:

- Added [`exec_service.rs`](../../crates/operond/src/exec_service.rs) so daemon
  exec RPC routing delegates out of [`operond/src/main.rs`](../../crates/operond/src/main.rs).
- Kept PTY/session runtime ownership in
  [`exec_session.rs`](../../crates/operond/src/exec_session.rs).
- Added [`grpc_exec.rs`](../../crates/operon-cli/src/grpc_exec.rs) for
  exec-specific CLI gRPC streaming helpers.
- Added [`scripts/verify-v0.10.4-maintainability-cleanup.sh`](../../scripts/verify-v0.10.4-maintainability-cleanup.sh)
  and wired it into CI and DEVELOPMENT.md.

Remaining:

- No v0.10.4 work remains.
- Broader onboarding and service forwarding decomposition remains future
  maintainability work if those surfaces grow.

## Phase 75: v0.11.2 Exec Session Hardening

Status: Completed.

Goal: tighten the PTY-backed exec session surface so interactive clients keep
terminal dimensions synchronized and dropped response streams do not leave
orphaned remote session processes.

Detailed plan: `docs/plan/v0.11.2-exec-session-hardening.md`.

Completed:

- `operon exec session` now uses the attached local TTY size by default when
  `--rows` or `--cols` are omitted.
- Interactive Unix sessions forward terminal resize signals as
  `ExecSessionResize` messages over `OpenExecSession`.
- Daemon response streams now terminate the remote session when dropped before
  a terminal exit event.
- Recorded `portable-pty` as the intended PTY abstraction for future macOS and
  Windows validation, with platform behavior deferred until Windows CI and
  packaging are defined.
- Added [`scripts/verify-v0.11.2-exec-session-hardening.sh`](../../scripts/verify-v0.11.2-exec-session-hardening.sh)
  and wired it into CI and DEVELOPMENT.md.

Remaining:

- No v0.11.2 work remains.
- macOS and Windows PTY validation remains future platform/distribution work.

## Phase 76: v0.10.5 Maintainability Cleanup

Status: Completed.

Goal: continue behavior-preserving modularization by moving service tunnel
state machines and CLI service transport helpers behind focused module
boundaries.

Detailed plan: `docs/plan/v0.10.5-maintainability-cleanup.md`.

Completed:

- Added [`service_tcp_forward.rs`](../../crates/operond/src/service_tcp_forward.rs)
  for daemon TCP service tunnel stream behavior.
- Added [`service_datagram_forward.rs`](../../crates/operond/src/service_datagram_forward.rs)
  for daemon UDP datagram tunnel session behavior.
- Kept [`service_forward.rs`](../../crates/operond/src/service_forward.rs)
  focused on service health, authorization, audit, target parsing, and
  delegation.
- Added [`grpc_service.rs`](../../crates/operon-cli/src/grpc_service.rs) for
  CLI service forwarding transport helpers.
- Added [`scripts/verify-v0.10.5-maintainability-cleanup.sh`](../../scripts/verify-v0.10.5-maintainability-cleanup.sh)
  and wired it into CI and DEVELOPMENT.md.

Remaining:

- No v0.10.5 work remains.

## Phase 77: v0.11.3 Platform Capability Matrix and CI Smoke

Status: Completed.

Goal: decide whether macOS and Windows can align with Linux for the core
daemon/CLI runtime before public release artifacts expand beyond Linux.

Current status: completed on 2026-05-04.

Detailed plan: `docs/plan/v0.11.3-platform-capability-matrix.md`.

Completed:

- Documented the platform capability matrix for Linux, macOS, and Windows.
- Added macOS and Windows Rust platform smoke CI for workspace checks and
  focused core tests.
- Kept public release artifacts Linux-only for now and documented macOS/Windows
  as candidate core runtime platforms.
- Kept Linux FUSE mount support Linux-only, with macFUSE and WinFsp deferred.
- Kept interactive exec sessions on the existing `portable-pty` abstraction.
- Added platform-specific shell defaults for command-string exec/session
  requests: Unix uses `/bin/sh -c`, Windows uses `cmd.exe /C`.
- Added [`scripts/verify-v0.11.3-platform-capability-matrix.sh`](../../scripts/verify-v0.11.3-platform-capability-matrix.sh)
  and wired it into CI and DEVELOPMENT.md.
- Follow-up CI repair authenticated `arduino/setup-protoc` with the workflow
  token, made preserved environment tests tolerate Windows `Path` casing, and
  updated the older v0.8.6 modularization verifier for the current
  service TCP/datagram module boundaries.

Remaining:

- No v0.11.3 work remains.
- macOS and Windows release artifacts were intentionally deferred out of
  v0.11.3 and are addressed by the v0.12 release/distribution phase.

## Phase 78: v0.12 Release / Distribution Readiness

Status: Completed.

Goal: make the public release surface match the v0.11.3 platform decisions, or
explicitly narrow the pre-1.0 supported target set so users do not infer
unsupported macOS or Windows parity.

Detailed plan: `docs/plan/v0.12-release-distribution-readiness.md`.

Planned:

- decide whether pre-1.0 releases remain Linux-only or expand to macOS and
  Windows core daemon/CLI preview binaries.
- if expanding, add release matrix jobs, packaging, checksums, and smoke
  validation for selected macOS and Windows targets.
- if staying Linux-only for now, update architecture and release docs so the
  target set is explicit rather than aspirational.
- keep release tag, Rust crate versions, TypeScript SDK version, CLI version,
  daemon version, and runtime health version aligned.
- keep README Quickstart and release validation scripts in sync with the
  supported target set.
- keep Linux FUSE mount support Linux-only and defer macFUSE/WinFsp adapter
  work.

Completed:

- Expanded public release automation from Linux-only to Linux plus macOS and
  Windows core runtime preview artifacts.
- Kept Linux release targets for `linux-x86_64`, `linux-arm64`, and
  `linux-armv7`.
- Added native preview release targets for `macos-x86_64`, `macos-aarch64`,
  and `windows-x86_64`.
- Kept Linux and macOS archives as `.tar.gz`; packaged Windows archives as
  `.zip`.
- Added native daemon/CLI version and help smoke checks for release artifacts.
- Updated README install instructions, Project Status, architecture docs, and
  quickstart validation target detection.
- Added [`scripts/verify-v0.12-release-distribution-readiness.sh`](../../scripts/verify-v0.12-release-distribution-readiness.sh)
  and wired it into CI and DEVELOPMENT.md.
- Bumped Rust crate versions, TypeScript SDK package version, CLI version, and
  runtime health `PROTOCOL_VERSION` to `0.12.2` / `v0.12.2` across the
  combined v0.12 through v0.12.2 implementation.

Remaining:

- No v0.12 work remains.
- Linux FUSE remains the only supported mount adapter.

## Phase 79: v0.12.1 Platform Parity Hardening

Status: Completed.

Goal: close the highest-risk macOS and Windows behavior gaps after the release
target decision, without claiming full parity for Linux-only mount or
`openat2(RESOLVE_BENEATH)` workspace containment.

Detailed plan: `docs/plan/v0.12.1-platform-parity-hardening.md`.

Planned:

- define Windows private token/config file semantics using ACL checks or clear
  diagnostics.
- assess and, if needed, implement Windows Job Object based exec process-tree
  cancellation.
- add macOS `portable-pty` interactive session smoke coverage and report
  Windows PTY validation as deferred until a runner-safe smoke path exists.
- extend `operon doctor` with platform caveats for mount support, private file
  permissions, exec cancellation, PTY validation, and firewall-sensitive
  service forwarding.
- update README, `PROTOCOL.md`, runtime API docs, architecture docs, and
  validation scripts for any changed platform guarantee.

Completed:

- Added platform-specific private-file diagnostics for Unix owner-only mode and
  Windows ACL warning semantics.
- Documented Windows non-interactive exec cancellation as direct-child
  best-effort until Job Object process-tree cancellation is implemented.
- Added daemon exec cancellation guarantee tests.
- Added `portable-pty` smoke validation for session start, resize, output, and
  exit behavior on Unix-like CI runners; Windows PTY validation is reported as
  deferred instead of blocking CI on a hanging smoke test.
- Extended `operon doctor` with platform caveats for mount support,
  private-file protection, exec cancellation, PTY validation, and service
  forwarding firewall sensitivity.
- Added [`scripts/verify-v0.12.1-platform-parity-hardening.sh`](../../scripts/verify-v0.12.1-platform-parity-hardening.sh)
  and wired it into CI and DEVELOPMENT.md.

Remaining:

- No v0.12.1 work remains.
- Windows Job Object process-tree cancellation remains future hardening.
- macFUSE and WinFsp mount adapters remain deferred.

## Phase 80: v0.12.2 Maintainability Cleanup

Status: Completed.

Goal: perform behavior-preserving cleanup around the remaining large daemon and
CLI surfaces after release/platform hardening, so future feature work stays
localized and easier to validate.

Detailed plan: `docs/plan/v0.12.2-maintainability-cleanup.md`.

Planned:

- reassess large-file hotspots, currently including
  [`operond/src/main.rs`](../../crates/operond/src/main.rs),
  [`operon-cli/src/grpc.rs`](../../crates/operon-cli/src/grpc.rs),
  [`operond/src/exec_session.rs`](../../crates/operond/src/exec_session.rs),
  [`operon-cli/src/commands/exec.rs`](../../crates/operon-cli/src/commands/exec.rs),
  and [`operon-cli/src/commands/config.rs`](../../crates/operon-cli/src/commands/config.rs).
- move remaining daemon RPC routing clusters behind focused service modules.
- continue extracting shared CLI gRPC request/stream helpers behind command
  family boundaries.
- split exec command/session UI responsibilities where terminal I/O, TTY
  sizing, resize forwarding, and rendering are still coupled.
- split config explain/render/init helper responsibilities where the current
  command file still mixes them.
- add maintainability validation coverage for the new module boundaries.

Completed:

- Added [`runtime.rs`](../../crates/operond/src/runtime.rs) for daemon gRPC
  runtime method routing and removed the `OperonRuntime` implementation from
  [`operond/src/main.rs`](../../crates/operond/src/main.rs).
- Added [`exec_args.rs`](../../crates/operon-cli/src/commands/exec_args.rs) for
  CLI exec shell/argv conversion and request construction.
- Added [`exec_session.rs`](../../crates/operon-cli/src/commands/exec_session.rs)
  for CLI PTY session UI, local TTY sizing, raw mode, resize forwarding, and
  result rendering.
- Added [`scripts/verify-v0.12.2-maintainability-cleanup.sh`](../../scripts/verify-v0.12.2-maintainability-cleanup.sh)
  and wired it into CI and DEVELOPMENT.md.

Remaining:

- No v0.12.2 work remains.
- [`operon-cli/src/grpc.rs`](../../crates/operon-cli/src/grpc.rs) and config
  command rendering remain future maintainability candidates if those surfaces
  grow.

## Phase 81: v0.12.3 Windows Exec Process-Tree Cancellation

Status: Completed.

Goal: bring Windows non-interactive exec cancellation closer to the Unix
process-group guarantee by validating and, if practical, implementing Job
Object based process-tree termination.

Detailed plan: `docs/plan/v0.12.3-windows-exec-process-tree-cancellation.md`.

Planned:

- design the smallest Windows Job Object integration needed for daemon exec
  process-tree cancellation.
- preserve Unix process-group behavior and existing non-Windows fallback
  behavior.
- add Windows-focused validation that cancellation terminates descendant
  processes, not only the direct child.
- update `operon doctor`, `PROTOCOL.md`, runtime API docs, README platform
  notes, and validation scripts with the implemented Windows guarantee.

Completed:

- Added a Windows-only Job Object guard in
  [`exec_runtime.rs`](../../crates/operond/src/exec_runtime.rs) for
  non-interactive exec cancellation and timeout termination.
- Preserved Unix process-group cancellation and non-Windows direct-child
  fallback behavior.
- Added Windows compile validation and a Windows-only descendant-process
  cancellation smoke test.
- Updated `operon doctor`, README, `PROTOCOL.md`, runtime API docs, CI, and
  validation scripts for the current Windows Job Object process-tree
  cancellation guarantee.
- Added
  [`scripts/verify-v0.12.3-windows-exec-process-tree-cancellation.sh`](../../scripts/verify-v0.12.3-windows-exec-process-tree-cancellation.sh).

Remaining:

- No v0.12.3 work remains.
- Windows private-file ACL enforcement and WinFsp mount support remain outside
  this phase.

## Phase 82: v0.12.4 Release Artifact Verification

Status: Completed.

Goal: close the loop between source-tree validation and public release
usability by verifying published GitHub Release artifacts after they are
created.

Detailed plan: `docs/plan/v0.12.4-release-artifact-verification.md`.

Planned:

- add a release-tag verification path that downloads public GitHub Release
  assets and validates `SHA256SUMS`.
- smoke-test extracted Linux, macOS, and Windows core runtime preview binaries
  from release artifacts.
- keep Linux FUSE and GLIBC baseline checks separate from macOS/Windows core
  runtime preview checks.
- keep README Quickstart artifact names and maintainer release validation
  commands aligned with the release workflow.

Completed:

- Added [`scripts/verify-release-artifacts.sh`](../../scripts/verify-release-artifacts.sh)
  to download public GitHub Release assets, verify `SHA256SUMS`, enforce the
  expected Linux/macOS/Windows/SDK asset set, and smoke-test the current
  platform archive.
- Added the manual `Verify Release Artifacts` GitHub Actions workflow for
  Linux, macOS, and Windows runners.
- Added
  [`scripts/verify-v0.12.4-release-artifact-verification.sh`](../../scripts/verify-v0.12.4-release-artifact-verification.sh)
  and wired it into CI and DEVELOPMENT.md.
- Updated release documentation so maintainers run
  `scripts/verify-release-artifacts.sh <tag>` after publishing.
- Follow-up workflow validation fixed the draft release workflow's Windows zip
  packaging command so the tag-triggered workflow parses before release jobs
  are scheduled.

Remaining:

- No v0.12.4 work remains.
- npm, crates.io, code-signing, notarization, and installer automation remain
  outside this phase.

## Phase 83: v0.12.5 CLI gRPC Maintainability Cleanup

Status: Completed.

Goal: continue behavior-preserving maintainability cleanup by splitting the
remaining large CLI gRPC helper surface into focused modules before more
protocol and operator features accumulate there.

Detailed plan: `docs/plan/v0.12.5-cli-grpc-maintainability-cleanup.md`.

Planned:

- reassess `crates/operon-cli/src/grpc.rs` responsibility clusters including
  channel construction, auth metadata, node selection, filesystem streams, exec
  streams, service tunnel helpers, and diagnostics helpers.
- move stable helper clusters behind focused CLI modules while preserving
  existing command output, JSON contracts, stream semantics, and error wording.
- colocate moved unit tests with their new modules.
- add maintainability validation coverage for the resulting CLI gRPC module
  boundaries.

Completed:

- Kept [`grpc.rs`](../../crates/operon-cli/src/grpc.rs) as the CLI gRPC
  compatibility and shared connection/context surface.
- Moved filesystem helpers into
  [`grpc_fs.rs`](../../crates/operon-cli/src/grpc_fs.rs).
- Moved non-session exec helper RPCs into
  [`grpc_exec_api.rs`](../../crates/operon-cli/src/grpc_exec_api.rs).
- Moved service list/check helpers into
  [`grpc_service_api.rs`](../../crates/operon-cli/src/grpc_service_api.rs).
- Moved audit listing into
  [`grpc_audit.rs`](../../crates/operon-cli/src/grpc_audit.rs).
- Added
  [`scripts/verify-v0.12.5-cli-grpc-maintainability-cleanup.sh`](../../scripts/verify-v0.12.5-cli-grpc-maintainability-cleanup.sh)
  and wired it into CI and DEVELOPMENT.md.
- Follow-up CI verification aligned the older v0.11 and v0.11.2 validation
  scripts with the v0.12.2 CLI exec session module split so those gates check
  [`commands/exec_session.rs`](../../crates/operon-cli/src/commands/exec_session.rs)
  instead of the pre-split exec command file.

Remaining:

- No v0.12.5 work remains.
- Config command rendering and deeper onboard splitting remain future
  maintainability candidates.

## Phase 84: v0.13 Release Publication and Public Verification

Status: Completed.

Goal: publish the current cross-platform preview release from `main` and verify
the public artifacts end to end instead of stopping at source-tree CI.

Detailed plan: `docs/plan/v0.13-release-publication.md`.

Completed:

- Published public GitHub Release
  [`v0.13.1`](https://github.com/denghongcai/Operon/releases/tag/v0.13.1)
  from release commit `e41309015f9765ea0a3ebd54dc539940c6ef9af9` after
  confirming `main`, `origin/main`, and the release tag all pointed at the same
  commit.
- Confirmed `CI` and `CodeQL` were green on the release commit before
  publication, and confirmed the tag-triggered `CI` run also passed.
- Confirmed the `Draft Release` workflow produced Linux x86_64, Linux arm64,
  Linux armv7, macOS x86_64, macOS aarch64, Windows x86_64, TypeScript SDK,
  and `SHA256SUMS` assets.
- Published the release and ran
  [`Verify Release Artifacts`](https://github.com/denghongcai/Operon/actions/runs/25316126490)
  against public tag `v0.13.1`; Linux, macOS, and Windows verification all
  passed.
- Ran README Quickstart release validation against public tag `v0.13.1` with
  [`scripts/verify-readme-quickstart-docker.sh`](../../scripts/verify-readme-quickstart-docker.sh).

Remaining:

- No v0.13 release publication work remains.
- npm, crates.io, code-signing, notarization, installers, and package manager
  automation remain outside this phase.

## Phase 85: v0.13.1 Windows PTY Validation

Status: Completed.

Goal: replace the current Windows PTY validation deferral with an explicit,
runner-safe decision: supported, degraded, or intentionally unsupported for the
current release line.

Detailed plan: `docs/plan/v0.13.1-windows-pty-validation.md`.

Planned:

- reproduce or isolate the Windows `portable-pty` smoke hang without allowing
  CI to hang indefinitely.
- decide whether Windows `exec session` is supported, degraded, or unsupported
  for this release line.
- update `operon doctor`, README, `PROTOCOL.md`, and runtime API docs with the
  exact Windows PTY status.
- add a Windows-safe validation path for the chosen status while preserving
  Unix-like PTY smoke coverage.

Completed:

- Chose an explicit `unsupported` status for Windows interactive exec sessions
  in this release line while preserving non-interactive Windows exec and Job
  Object cancellation support.
- Added a Windows `UNIMPLEMENTED` daemon response before opening PTY sessions
  so clients fail clearly instead of hanging.
- Updated `operon doctor` to report `windows-exec-session-unsupported` on
  Windows, with Unix-like platforms still reporting validated `portable-pty`
  smoke coverage.
- Added a Windows-safe CI test for the unsupported decision and kept Unix-like
  `portable-pty` smoke validation.
- Updated README, `PROTOCOL.md`, runtime API docs, architecture docs, and
  [`scripts/verify-v0.13.1-windows-pty-validation.sh`](../../scripts/verify-v0.13.1-windows-pty-validation.sh).

Remaining:

- No v0.13.1 Windows PTY validation work remains.
- Replacing `portable-pty`, adding a Windows-specific PTY backend, and
  redesigning terminal UX remain outside this phase.

## Phase 86: v0.13.2 Windows Private File ACL Enforcement

Status: Completed.

Goal: move Windows token/config private-file handling from warning-only
diagnostics to real ACL-aware validation where practical.

Detailed plan: `docs/plan/v0.13.2-windows-private-file-acl.md`.

Completed:

- Defined the Windows ACL conditions Operon treats as private enough for
  token/config private files: current user, Administrators, and SYSTEM may have
  access; missing DACLs or access grants to other trustees are rejected.
- Added Windows ACL inspection and protected ACL application for private files
  generated by CLI initialization and onboarding flows.
- Updated `operon doctor` to report `windows-acl-verified`.
- Added Windows-focused ACL model tests, Windows-only private-file write smoke
  coverage in CI, and
  [`scripts/verify-v0.13.2-windows-private-file-acl.sh`](../../scripts/verify-v0.13.2-windows-private-file-acl.sh).
- Updated README, `PROTOCOL.md`, runtime API docs, and platform architecture
  docs with the implemented guarantee.

Remaining:

- No v0.13.2 Windows ACL work remains.
- Secret-manager integration and general Windows permission management remain
  outside this phase.

## Phase 87: v0.13.3 Config and Onboard Maintainability Cleanup

Status: Completed.

Goal: reduce future config UX change cost by splitting the remaining config and
onboard command responsibilities into clearer plan, render, and write
boundaries.

Detailed plan: `docs/plan/v0.13.3-config-onboard-maintainability.md`.

Completed:

- Split `operon config explain` execution and text rendering into
  [`commands/config/explain.rs`](../../crates/operon-cli/src/commands/config/explain.rs)
  while keeping
  [`commands/config.rs`](../../crates/operon-cli/src/commands/config.rs) as the
  data model and compatibility export boundary.
- Added explicit onboarding plan, render, and write module boundaries in
  [`onboard/plan.rs`](../../crates/operon-cli/src/onboard/plan.rs),
  [`onboard/render.rs`](../../crates/operon-cli/src/onboard/render.rs), and
  [`onboard/write.rs`](../../crates/operon-cli/src/onboard/write.rs).
- Preserved generated config shape, token-file behavior, CLI text output, JSON
  summary output, and existing error contracts through focused tests.
- Added
  [`scripts/verify-v0.13.3-config-onboard-maintainability.sh`](../../scripts/verify-v0.13.3-config-onboard-maintainability.sh)
  and wired it into consolidated CI validation.

Remaining:

- No v0.13.3 config/onboard cleanup work remains.
- Config schema redesign, endpoint/discovery behavior changes, and new
  onboarding product flows remain outside this phase.

## Phase 88: v0.13.4 CI Validation Consolidation

Status: Completed.

Goal: reduce GitHub Actions job sprawl by consolidating version-scoped
validation jobs into a small grouped validation matrix while keeping each
validation script separate and easy to maintain.

Detailed plan: `docs/plan/v0.13.4-ci-validation-consolidation.md`.

Planned:

- add a single runner script that executes existing `scripts/verify-*.sh`
  validation scripts in stable order with GitHub Actions log grouping.
- replace the version-scoped validation matrix with grouped `Validation` jobs
  for `core`, `runtime`, `sdk`, and `linux-system`.
- collect validation failures and report a final summary instead of stopping at
  the first failing script.
- document that future version validation scripts extend the consolidated
  runner and choose an existing group instead of adding new version-specific
  matrix jobs unless a distinct OS, permission model, service container, or
  trigger is required.
- skip duplicate `@operon/sdk` unit tests in validation CI after the separate
  `TypeScript` job has already run `pnpm -r test`.

Completed:

- Added [`scripts/ci/run-validations.sh`](../../scripts/ci/run-validations.sh)
  as the consolidated validation runner.
- Replaced the `.github/workflows/ci.yml` version validation matrix with four
  grouped `Validation` jobs while preserving separate `Rust`, `TypeScript`, and
  platform smoke jobs.
- Set validation CI to use `OPERON_SKIP_SDK_TESTS=1`, avoiding duplicate SDK
  unit-test runs after the `TypeScript` job while keeping local validation
  scripts independently runnable with SDK tests enabled by default.
- Updated contributor and agent guidance for future version validation
  additions.

Remaining:

- No v0.13.4 CI validation consolidation work remains.
- Release-draft and release-artifact verification workflows remain separate.

## Phase 89: v0.13.5 Daemon Service Management

Status: Completed.

Goal: give `operond` a first-class managed daemon setup path without adding a
traditional `operond start --background` mode.

Detailed plan: `docs/plan/v0.13.5-daemon-service-management.md`.

Planned:

- add `operond service install/start/stop/status/uninstall`.
- keep `operond start` as the foreground runtime command and avoid
  `operond start --background`.
- support platform-native supervision: Linux systemd user units, macOS launchd
  user agents, and a Windows Service entrypoint that speaks the Service Control
  Manager protocol.
- generate service definitions that call `operond start --config <path>` with
  explicit config paths and no embedded secrets.
- document service management in README, DEVELOPMENT, AGENTS, and validation
  guidance.

Completed:

- Added `operond service install/start/stop/status/uninstall`.
- Kept `operond start` as the foreground runtime command with no
  `--background` flag.
- Added Linux user-level systemd unit installation/control.
- Added macOS launchd user-agent installation/control.
- Added a Windows Service Control Manager entrypoint through
  `operond service run --config <path>` instead of pretending the foreground
  `operond start` command is a Windows Service.
- Added service definition rendering tests, command-surface tests, and
  `scripts/verify-v0.13.5-daemon-service-management.sh`.
- Added platform smoke CI coverage for daemon service-management tests on
  macOS and Windows.
- Updated README, DEVELOPMENT, AGENTS, and
  `docs/plan/v0.13.5-daemon-service-management.md`.

Remaining:

- No v0.13.5 daemon service management work remains.
- Background self-daemonization remains outside the product model.
- Elevated Windows install/start/stop smoke remains a manual pre-release check
  when practical because CI runners may not allow persistent service
  registration.

## Phase 90: v0.13.6 Test Hardening

Status: Completed.

Goal: turn the recent test-coverage review into targeted, high-signal coverage
for the parts of Operon that are most likely to regress silently: Linux mount
adapter behavior, network service checks, shared gRPC client helpers, CLI
negative paths, and test cleanup reliability.

Detailed plan: `docs/plan/v0.13.6-test-hardening.md`.

Planned:

- add focused mount adapter tests for error mapping and testable FUSE or remote
  client behavior while keeping live kernel mount behavior in Linux validation
  scripts.
- add deterministic TCP/UDP service-check coverage and fix the TCP success
  reason text that currently uses UDP wording.
- add `operon-grpc-client` chunk-boundary, metadata, and connection-deadline
  coverage.
- extend compiled-binary CLI integration tests with representative negative
  paths.
- replace targeted manual temporary-directory cleanup with RAII cleanup.
- remove duplicate token-generation coverage by making onboard tests assert
  onboard-specific behavior.
- refresh `docs/quality/test-coverage-audit.md` to match current coverage and
  the gaps closed by this phase.

Completed:

- Added deterministic TCP/UDP service-check coverage in
  [`operon-network`](../../crates/operon-network) and fixed the TCP success
  reason string.
- Followed up on CI validation by preserving the explicit UDP socket-connect
  caveat on successful UDP checks while keeping the corrected TCP success
  reason.
- Added `operon-grpc-client` chunk-boundary, metadata, and connection-deadline
  coverage, and routed the Linux mount remote client through the shared deadline
  helper.
- Added `operon-mount` focused tests for errno mapping and FUSE helper behavior
  that can be tested without a live kernel mount.
- Added compiled-binary CLI negative-path tests for clap errors, missing
  arguments, malformed config, and invalid endpoint schemes.
- Replaced targeted manual temporary-directory cleanup with `tempfile::TempDir`.
- Replaced duplicate onboard token-generation coverage with onboard-specific
  token-file/config-reference coverage.
- Refreshed [`docs/quality/test-coverage-audit.md`](../quality/test-coverage-audit.md).
- Added
  [`scripts/verify-v0.13.6-test-hardening.sh`](../../scripts/verify-v0.13.6-test-hardening.sh)
  and wired it into consolidated CI validation.

Remaining:

- No v0.13.6 test-hardening work remains.
- Numeric line-coverage thresholds, macFUSE, WinFsp, and CLI UX redesign remain
  outside this phase.

## Phase 91: v0.13.7 Mount Adapter Strategy

Status: Completed.

Goal: decide whether and how Operon should pursue macFUSE and WinFsp mount
adapters without blurring the existing boundary between core filesystem RPCs
and platform-specific live mount integrations.

Detailed plan: `docs/plan/v0.13.7-mount-adapter-strategy.md`.

Completed:

- Documented macFUSE and WinFsp dependency, permission, packaging, and CI
  implications.
- Kept mount adapters framed as optional convenience layers over the Core FS
  Protocol unless a separate product decision changes that boundary.
- Compared implementation paths: continue Linux-only mount support, macFUSE
  first, WinFsp first, or shared adapter abstraction first.
- Defined validation expectations for read, write, range read, directory
  listing, error mapping, cancellation, and permission-denied behavior.
- Decided supported live mounts remain Linux-only before v1.0.
- Decided the next mount implementation phase should extract a shared
  mount-core boundary before any macFUSE or WinFsp adapter.
- Decided macFUSE FSKit is the first experimental non-Linux adapter candidate,
  while macFUSE kernel backend stays a manual fallback and WinFsp should prefer
  the native API after packaging and license review.

Remaining:

- No v0.13.7 strategy work remains.
- macFUSE and WinFsp implementation remain outside this strategy phase.
- Shared mount-core extraction was completed later in v0.13.8.

## Phase 92: v0.13.8 Mount Core Boundary

Status: Completed.

Goal: extract the platform-neutral mount adapter boundary before attempting
macFUSE FSKit or WinFsp native implementation.

Detailed plan: `docs/plan/v0.13.8-mount-core-boundary.md`.

Completed:

- Added `crates/operon-mount/src/mount_core.rs` as the platform-neutral mount
  boundary for `RemoteFs`, remote path normalization, child-name validation, and
  child path joining.
- Moved the public `RemoteFs` contract out of the gRPC client module and made
  the gRPC-backed client implement `mount_core::RemoteFs`.
- Removed the crate-root Linux gate from `operon-mount` while keeping Linux
  FUSE adapter modules, inode table, and session management behind Linux
  `cfg` gates.
- Preserved current Linux FUSE behavior while making mount-core unit tests
  runnable without a live kernel mount.
- Added `scripts/verify-v0.13.8-mount-core-boundary.sh` and wired it into the
  consolidated validation runner.
- Kept macFUSE FSKit and WinFsp native adapter implementation deferred.
- Aligned the public release line to `0.13.8` / `v0.13.8` across Rust crate
  versions, the TypeScript SDK package version, `PROTOCOL_VERSION`, CLI version
  tests, and validation scripts before publication.
- Published `v0.13.8` from the `main` commit
  `71956cfdde79fb5ba1c9497bd9c19f7a19664762`.
- Validated the release with successful main CI, main CodeQL, tag CI, Draft
  Release asset generation, post-publication artifact verification on Ubuntu,
  macOS, and Windows, and README Quickstart Docker validation against the
  public `v0.13.8` release.

Remaining:

- No v0.13.8 mount-core boundary work remains.
- macFUSE and WinFsp implementation remain outside this phase.

## Phase 93: v0.14 Cross-Platform Live Mount

Status: In progress.

Goal: make live mount a complete core Operon capability across Linux, macOS,
and Windows instead of treating non-Linux mount support as deferred convenience
work.

Detailed plan: `docs/plan/v0.14-cross-platform-live-mount.md`.

macOS release-gate runbook:
`docs/plan/v0.14-macos-live-smoke-runbook.md`.

Completed:

- Supersede the v0.13.7 Linux-only pre-v1.0 support decision for live mounts.
- Expand the shared `mount_core` boundary so platform adapters reuse common
  path validation, operation mapping, stat/list/read/write/truncate,
  mkdir/delete/rename semantics, error classification, and cancellation tests.
- Convert the Linux-only FUSE adapter boundary into a Unix FUSE adapter that
  preserves Linux behavior and enables macOS/macFUSE implementation.
- Add macOS live mount support with compile/unit validation and a live smoke
  gate for macFUSE runtime behavior.
- Add Windows live mount support through a native WinFsp adapter, using an
  Apache-2.0-compatible dependency path such as MIT-compatible bindings or
  direct FFI rather than adding GPLv3 bindings without an explicit license
  decision.
- Replace the current non-Linux `operon mount` error with platform-aware mount
  support and missing-runtime diagnostics.
- Extend `operon doctor`, README, PROTOCOL, runtime API docs, repo-local
  skills, AGENTS guidance, validation scripts, and release documentation to
  match the implemented mount support boundary.
- Preserve Linux FUSE inode mappings across rename callbacks so kernel dentries
  remain valid for post-rename `stat` and `unlink` operations.
- Align Rust crate versions, the TypeScript SDK package version, CLI version
  output, and `PROTOCOL_VERSION` to `0.14.0` / `v0.14.0`.
- Add a manual Actions live-smoke workflow for macOS FUSE-T and Windows WinFsp
  validation.
- Validate the current implementation checkpoint with the `core`, `runtime`,
  and `linux-system` consolidated validation groups.
- Validate the latest pushed checkpoint with green remote CI across Rust,
  TypeScript, macOS/Windows platform smoke, and every consolidated validation
  group.
- Harden mount-session shutdown handling so non-interactive runner contexts
  that cannot install a Ctrl-C handler or lose the shutdown channel do not
  immediately terminate live mount processes, and expand Windows live-smoke
  diagnostics for daemon, mount, process, and WinFsp service state.
- Initialize WinFsp through `winfsp_wrs::init()` before starting the Windows
  adapter so the delayed WinFsp DLL is loaded from the installed runtime
  directory before the first WinFsp API call.
- Add Windows-only `winfsp_wrs_build` build scripts for `operon-cli` and
  `operon-mount` so the MSVC linker marks the WinFsp DLL as delayed-load; this
  lets `winfsp_wrs::init()` run before the loader resolves the installed WinFsp
  runtime DLL.
- Add opt-in Windows mount callback tracing through `OPERON_MOUNT_TRACE` and
  expand the Windows live-smoke diagnostics to print drive and mounted-root
  state after the mounted process starts but does not expose expected files.
- Harden Windows live-smoke diagnostics so native `dir` / drive-probe failures
  do not suppress daemon and mount callback logs; the latest evidence shows the
  WinFsp drive letter exists but root directory access returns `Incorrect
  function`.
- Add WinFsp volume-info and dispatcher-stopped callback trace points plus
  `mountvol` / `fsutil fsinfo drivetype` / `fsutil fsinfo volumeinfo`
  diagnostics so the next live-smoke run can separate mount-manager exposure
  from adapter callback dispatch and volume metadata failures.
- Enable WinFsp double buffering in the Windows volume parameters after live
  smoke showed fixed-drive registration but no adapter callback dispatch for
  volume information, and add per-job/live-script timeouts so hung macOS or
  Windows live-smoke runs fail with diagnostics instead of blocking release
  validation.
- Add a diagnostic-only `operon-mount/winfsp-debug` feature and enable it in the
  Windows live-smoke build after double buffering did not change behavior; this
  keeps normal release builds quiet while letting the next failed smoke include
  WinFsp dispatcher/debug output.
- Fix Windows WinFsp `Create` handling for existing paths after debug output
  showed root opens arrive as `Create "" FILE_OPEN`; the adapter now returns a
  context for existing paths before creating missing files or directories.
- Expose the Windows WinFsp `CreateEx` callback and delegate it to the same
  existing-path open-or-create logic after live-smoke diagnostics showed root
  opens still returned `STATUS_INVALID_DEVICE_REQUEST` before entering adapter
  `create` or `open` trace points.
- Harden completion validation in the v0.8.1 integration coverage and v0.8
  agent-skills scripts so completion output is written to temporary files before
  `grep` assertions, avoiding `clap_complete` broken-pipe panics in CI.
- Add the next live-smoke diagnostic checkpoint after the latest manual run:
  macOS now dumps diagnostics immediately when the seed file is not exposed and
  uses bounded process cleanup instead of waiting indefinitely for a stuck mount
  process; Windows now logs callback flags and callback entry before remote
  filesystem calls, reduces the seed-exposure wait window, and narrows create
  dispatch to WinFsp `CreateEx` so the next failure clearly shows whether
  dispatch reaches the adapter.
- Fix the macOS live-smoke diagnostic helper so it runs `set +e` in a subshell;
  the latest live-smoke evidence showed diagnostics were disabling `errexit`
  globally and could let a missing seed file continue into later test steps.
  Windows live-smoke evidence now shows callback flags are present but WinFsp
  still returns `STATUS_INVALID_DEVICE_REQUEST` before any adapter callback.
- Align the Windows WinFsp adapter with conservative official sample-style
  volume/interface defaults after live-smoke diagnostics showed registered
  callback flags but no adapter callback dispatch: use case-insensitive search,
  persistent ACLs,
  post-cleanup-when-modified-only, and an explicit volume creation time while
  retaining double buffering.
- Re-enable the Windows WinFsp `CreateEx` callback after local binding
  inspection showed the current WinFsp interface includes `CreateEx` and newer
  Rust host wrappers dispatch creates through that trampoline; the adapter now
  exposes both `Create` and `CreateEx` while delegating both to the same
  existing-path open-or-create behavior.
- Add a `platform=all|macos|windows` input to the manual v0.14 live-smoke
  workflow so Windows WinFsp checkpoints can run independently while the macOS
  hosted-runner FSKit entitlement/registration blocker remains unresolved.
- Narrow the latest macOS live-smoke failure to the FUSE mount/handshake
  boundary: daemon and mount process stay alive, but the mountpoint never enters
  the system mount table and CLI output never reaches the post-mount line. The
  macOS smoke script now attempts macFUSE kernel-extension loading, records
  macFUSE/kext readiness diagnostics, and enables Unix mount trace points around
  remote connection, root stat, and `fuser::spawn_mount2`.
- Select macFUSE's `backend=fskit` mount option by default on macOS 15.4+ after
  live-smoke evidence showed GitHub-hosted macOS 15.7.4 installs macFUSE 5.2.0
  but does not load the kernel extension, leaving `spawn_mount2` hung before the
  mount completes. `OPERON_MOUNT_MACOS_BACKEND=kernel` remains available for
  explicit kernel-backend validation.
- Move macOS FSKit live-smoke mount points under `/Volumes` and reject FSKit
  mount requests outside `/Volumes` with an actionable error after macFUSE
  documentation and CI evidence showed `/var/folders/.../mount` cannot work for
  the FSKit backend.
- Refresh macFUSE's file-system-extension component before FSKit live smoke and
  dump recent FSKit/LiveFS/macFUSE unified logs after `/Volumes` smoke still
  hung in `fuser::spawn_mount2`, narrowing the remaining macOS issue to the
  FSKit service-registration/XPC layer.
- Record the hosted macOS FSKit blocker: refreshed FSKit smoke reaches the
  macOS service layer, but unified logs report `Hello FSClient! entitlement no`
  followed by macFUSE daemon mount and server-advertise failures. Remaining
  macOS live validation requires either a runner-safe entitlement/registration
  path or a host where the macFUSE kernel backend is approved and loaded.
- Switch the Windows WinFsp adapter from the `winfsp_wrs` high-level host to
  direct `winfsp_wrs_sys` interface registration after targeted Windows
  live-smoke logs showed the drive was registered but root opens still returned
  `STATUS_INVALID_DEVICE_REQUEST` before any adapter callback trace. The direct
  adapter now owns callback entry tracing and explicit WinFsp
  dispatcher/context/interface cleanup.
- Add direct WinFsp status and debug logging after the first direct-interface
  smoke still registered `O:\` as a fixed drive but returned `Incorrect
  function` for `fsutil volumeinfo O:\` and `dir O:\` before any adapter
  callback entry. The next targeted Windows smoke should include
  `FspFileSystemCreate`, mount-point, dispatcher, and WinFsp debug output.
- Register the Windows WinFsp `Overwrite` callback after direct-interface
  diagnostics showed `FspFileSystemCreate`, mount-point registration,
  dispatcher startup, and all create/open callback slots were present, but
  WinFsp v2.1 still returned `STATUS_INVALID_DEVICE_REQUEST` before adapter
  callback entry. The root cause is WinFsp's create/open dispatch precondition:
  `Create/CreateEx`, `Open`, and `Overwrite/OverwriteEx` must all be registered
  before `FspFileSystemOpCreate` will dispatch root opens.
- Validate Windows live mount on GitHub-hosted `windows-latest` for commit
  `b14a4bd` in run `25339076348`; the smoke covered drive exposure, read,
  write, truncate, mkdir, rename, delete, remote read-back, and cleanup through
  the WinFsp adapter.
- Re-run macOS live smoke on GitHub-hosted `macos-latest` for commit
  `b14a4bd` in run `25339408571` and confirm the remaining failure is still the
  macFUSE FSKit/LiveFS service boundary: Operon reaches `spawn_mount2_start`,
  while unified logs report an unentitled FSKit client followed by macFUSE
  daemon mount and server-advertise failures.
- Add a `macos_backend=fskit|kernel` workflow input for the manual v0.14
  live-smoke workflow so macOS validation can explicitly target the hosted
  runner FSKit path or a kernel backend on a host where the macFUSE kernel
  extension is approved and loaded.
- Run the manual `macos_backend=kernel` workflow on GitHub-hosted
  `macos-latest` for commit `9d3c4df` in run `25340391127`; it failed during
  the macOS smoke step after macFUSE installation reported the kernel-extension
  approval requirement, and GitHub did not publish the failing step body in the
  job log. The workflow now wraps the macOS smoke script with explicit
  stdout/stderr tee logging, prints the smoke exit code, and uploads the smoke
  log artifact on success or failure.
- Re-run the artifact-backed `macos_backend=kernel` workflow for commit
  `4160b0c` in run `25340798030`; the artifact confirmed GitHub-hosted macOS
  keeps the macFUSE kernel extension unloaded, Operon reaches
  `spawn_mount2_start`, and the seed file is never exposed. That run also
  showed smoke cleanup could wait without a bound for the stuck mount process
  until job timeout, so the macOS smoke script now uses a shorter default
  watchdog and bounded process cleanup waits.
- Re-run the bounded-cleanup `macos_backend=kernel` workflow for commit
  `1ed85f2` in run `25341745841`; it now fails cleanly with an uploaded
  artifact and explicit `macOS live mount smoke exit code: 1`, while preserving
  the same root-cause evidence: the hosted runner does not load the macFUSE
  kernel extension, the seed file is not exposed, and Operon reaches
  `spawn_mount2_start`.
- Replace the earlier `macos_runner=hosted|self-hosted-macfuse` manual
  live-smoke path with the FUSE-T
  `macos_runner=hosted|self-hosted-fuse-t` path. The hosted path is now the
  first release-gate candidate, while the self-hosted path targets a runner
  labeled `self-hosted`, `macOS`, and `fuse-t` where FUSE-T is already
  installed.
- Check the repository Actions runner registry before dispatching the
  self-hosted macOS live-smoke lane. The registry currently reports
  `total_count: 0`, so no `self-hosted`/`macOS`/`macfuse` runner is available
  and no queued self-hosted workflow run was created.
- Replace the earlier self-hosted macFUSE preflight with
  `scripts/preflight-v0.14-macos-fuse-t-host.sh` so a macOS live-smoke host
  fails early when FUSE-T is missing, `pkg-config fuse` is unavailable, or the
  selected backend is unsupported.
- Add `docs/plan/v0.14-macos-live-smoke-runbook.md` with the concrete
  host-preflight, self-hosted runner labels, workflow dispatch command, success
  evidence, and failure-log handling needed to execute the remaining macOS
  release gate once a suitable host is available.
- Add a tag-triggered release workflow guard through
  `scripts/verify-v0.14-release-gates.sh` so `v0.14*` release drafts fail
  before artifact builds unless the exact release commit has a successful
  macOS FUSE-T live-smoke run.
- Clarify the macOS live-smoke runbook with the concrete hosted-runner failure
  evidence: FSKit reaches the macFUSE service layer but lacks the required
  entitlement, while the kernel backend reports an unloaded macFUSE kernel
  extension and never exposes the seed file.
- Switch the active macOS live-smoke target from macFUSE to FUSE-T after
  hosted-runner macFUSE attempts showed runtime-level blockers. The macOS
  adapter keeps the existing `fuser` implementation path, defaults to
  FUSE-T's NFS backend, the workflows install
  `macos-fuse-t/homebrew-cask/fuse-t`, and release gates now accept a
  successful macOS FUSE-T hosted or self-hosted live mount job on the exact
  release commit.
- Run the first hosted FUSE-T smoke for commit `1c086ae` in run `25355056996`;
  FUSE-T 1.2.1 installed and provided `/usr/local/lib/libfuse-t.dylib`, but no
  `fuse.h` was present, so the install helper now creates link-only
  `pkg-config fuse` compatibility metadata when headers are absent.
- Run the follow-up hosted FUSE-T smoke for commit `a3409c6` in run
  `25355152823`; the smoke reached the built `operon` binary, but dyld aborted
  because `@rpath/libfuse-t.dylib` could not be resolved. The install helper
  now emits an rpath in generated pkg-config metadata and exports
  `DYLD_LIBRARY_PATH` for the smoke environment.
- Run the next hosted NFS smoke for commit `acc1e20` in run `25355546910`; the
  mount command reached `spawn_mount2_ok`, but the seed file was not exposed
  and unified logs reported `nfs_connect: socket connect taking a while for
  fuse-t:/...`. The next checkpoint adds deeper FUSE-T process/socket
  diagnostics and runs an SMB-backend smoke to isolate NFS-specific behavior.
- Run the hosted SMB smoke for commit `acc1e20` in run `25355950126`; it also
  reached `spawn_mount2_ok` without exposing the seed file, so the current
  evidence points at hosted macOS network-volume publication after FUSE-T
  starts, not at Operon's FUSE callback startup. The install helper now probes
  FUSE-T pkg-config metadata from both `/usr/local` and
  `/Library/Application Support/fuse-t/pkgconfig` before generating the
  compatibility `fuse.pc`.
- Run the final hosted NFS diagnostic smoke for commit `189be7e` in run
  `25356391309`; FUSE-T metadata was valid, `spawn_mount2_ok` was reached, and
  diagnostics showed `/sbin/mount_nfs` stuck connecting to
  `fuse-t:/operon-v014-macos-live-mount-*` while the FUSE-T `go-nfsv4` server
  listened on `127.0.0.1:<port>` with a `CLOSE_WAIT` connection. This narrows
  the remaining hosted blocker to FUSE-T/macOS network-volume connection after
  Operon's adapter has started. The smoke cleanup now bounds `umount` and
  mount-directory removal so failed hosted attempts do not linger during
  cleanup.
- Review the FUSE-T wiki against the current macOS mount path and add
  `OPERON_MOUNT_MACOS_OPTIONS` for comma-separated FUSE-T `-o` diagnostics such
  as `nobrowse` and `noattrcache`. The workflow now accepts a `macos_options`
  dispatch input, the smoke logs selected options and tails
  `~/Library/Logs/fuse-t`, and unit tests cover backend-plus-extra option
  construction plus rejection of raw `-d`/`-l` style arguments that `fuser`
  cannot pass as standalone parameters.
- Add a hosted macOS FUSE-T reference probe using
  `https://github.com/macos-fuse-t/fuse-zip` so v0.14 diagnostics can validate
  whether GitHub-hosted macOS can publish a non-Operon FUSE-T mount. The probe
  builds fuse-zip on the runner, mounts a seed ZIP through FUSE-T, checks
  seed-file read visibility, and uploads FUSE-T logs on failure.
- The first fuse-zip probe dispatch for commit `4b847e0` failed before mount
  validation because fuse-zip's `make release` target dropped the FUSE include
  directory and could not find `fuse.h`. The probe now passes explicit
  `pkg-config` cflags/libs for FUSE-T and libzip so the next run can test the
  actual hosted macOS FUSE-T mount path.
- The follow-up fuse-zip probe dispatch for commit `78d1061` in run
  `25358498739` compiled fuse-zip and reached the mounted `seed.txt` read path,
  then failed only because the probe also asserted third-party fuse-zip
  write-back persistence for `new.txt`. The probe is now narrowed to the
  intended hosted-runner control: FUSE-T mount publication and read visibility.
- The next fuse-zip probe dispatch for commit `f13e32b` in run `25358898425`
  printed `v0.14 macOS FUSE-T fuse-zip probe passed`, proving hosted FUSE-T
  read exposure through fuse-zip. The job was cancelled only because the
  probe's shell watchdog left an orphan `sleep` process holding the Actions
  output pipe open after success. The probe watchdog now uses a single `perl`
  timer process that cleanup can terminate without orphaning a child process.
- The corrected fuse-zip probe passed on GitHub-hosted `macos-latest` for
  commit `5f0a0bc` in run `25359068416` in 43 seconds. This proves the hosted
  runner can publish and read at least one FUSE-T NFS mount through an
  independent libfuse implementation, so the remaining Operon macOS live-smoke
  failure is no longer a blanket hosted-runner FUSE-T limitation.
- Mirror fuse-zip's low-risk FUSE compatibility callbacks in Operon's Unix FUSE
  adapter: known-inode `access()` now succeeds, `fsyncdir()` succeeds,
  `statfs()` returns stable non-zero capacity metadata, and
  `OPERON_MOUNT_TRACE` logs lookup/getattr/open/read/write/readdir/access/
  statfs/fsyncdir callback entry for the next hosted macOS live-smoke run.
- Mirror the fuse-zip probe watchdog in the Operon macOS live-smoke script:
  the smoke now uses a single `perl` timer process instead of a background
  shell plus `sleep`, so timeout/failure diagnostics can finish and upload
  artifacts without orphaned sleep processes holding the Actions output pipe.
- Use a platform-aware FUSE session thread count after hosted macOS run
  `25359328268` showed `spawn_mount2_ok`, no FUSE callback trace, and
  FUSE-T's NFS bridge stuck in `CLOSE_WAIT`: `fuser` 0.17 rejects
  `n_threads != 1` on non-Linux session loops, so Operon now keeps four FUSE
  threads on Linux and uses one thread on macOS.
- Extend the Unix FUSE adapter's fuse-zip-compatible macOS callback surface
  after hosted run `25359740331` reached `statfs` and root `getattr` with
  `n_threads: 1` but closed the FUSE-T NFS bridge before lookup/readdir:
  init/destroy/opendir/releasedir are now traced, `statfs` reports a non-zero
  fragment size, and xattr probes return empty/no-xattr responses instead of
  default `ENOSYS`.
- Align macOS mount options with the successful fuse-zip control after hosted
  run `25359898168` still closed the FUSE-T NFS bridge after root
  `statfs/getattr`: macOS now passes only FUSE-T-specific backend/extra options,
  while Linux keeps the existing `fsname`, `subtype`, `nodev`, `nosuid`, and
  `noexec` options.
- Continue the fuse-zip comparison after hosted run `25360049773` showed the
  minimized macOS option set still closes the FUSE-T NFS bridge immediately
  after root `statfs/getattr`: Operon now reports FUSE attributes with
  fuse-zip-compatible 512-byte stat blocks, reports macOS file ownership as the
  mounting user instead of root, logs the exact root attr/statfs values, and
  captures `mount_nfs`/NFS unified logs plus `nfsstat -m` in the macOS smoke
  diagnostics.
- Use hosted run `25360306048` evidence to remove the next root-attribute
  difference from fuse-zip/fuser hello: uid/gid and `blksize=512` reached the
  runner, but the root directory still reported `blocks=1`, so directories now
  report zero allocated stat blocks and `statfs` free-inode metadata is aligned
  to fuse-zip's zero value.
- Add a standalone hosted macOS FUSE-T `fuser` hello probe after hosted run
  `25360430340` confirmed root attributes now match the fuser hello/fuse-zip
  shape but the session still closes before `lookup` or `readdir`. This probe
  separates Operon filesystem semantics from the `fuser` low-level session path;
  if it fails the same way, the remaining macOS path should move toward a
  libfuse-style high-level adapter instead of further callback micro-adjustment.
- Record the hosted `fuser` hello probe result from run `25360630179`: a
  minimal `fuser` filesystem failed with the same `init` -> root
  `statfs/getattr` -> `destroy` sequence and no `lookup/readdir`, while FUSE-T
  logged `Connection closed`. This confirms the macOS blocker is the `fuser`
  low-level session path against FUSE-T's NFS bridge; the next implementation
  checkpoint is a fuse-zip-style libfuse high-level macOS adapter while Linux
  stays on `fuser`.
- Continue the fuser/FUSE-T root-cause check on branch
  `investigate-macos-fuser-fuset` with a workflow-controlled fuser hello
  experiment that patches fuser 0.17's macOS `INIT_FLAGS` down to
  `ASYNC_READ` only. This isolates whether the early FUSE-T disconnect is
  caused by fuser advertising macOS capabilities (`CASE_INSENSITIVE`,
  `VOL_RENAME`, `XTIMES`) that libfuse high-level filesystems only request when
  matching operations are implemented.
- Record hosted run `25375413405` from that diagnostic branch: patching fuser's
  macOS `INIT_FLAGS` changed the negotiated FUSE-T flags from `0xe0000001` to
  `0x40000001`, but the minimal fuser filesystem still failed with the same
  early `Connection closed`. The remaining bit 30 is produced by fuser's
  low-level init reply adding `FUSE_INIT_EXT`, which collides with macOS
  `FUSE_VOL_RENAME`; this supports moving the macOS adapter toward libfuse
  high-level mounting rather than continuing Operon callback adjustments.
- Add an Operon-local `fuser` 0.17.0 patch under
  `vendor/fuser-0.17.0-operon` and wire it through workspace
  `[patch.crates-io]`. The patch is macOS-scoped: keep default init flags at
  `FUSE_ASYNC_READ` and do not add Linux `FUSE_INIT_EXT` in macOS init replies,
  while preserving the upstream Linux handshake path. This is the next minimal
  live-smoke experiment before deciding whether the final macOS adapter can
  remain on patched `fuser` or must move to libfuse high-level mounting.
- Record hosted run `25375851519`: the vendored fuser handshake patch reduced
  FUSE-T negotiated flags to `0x00000001`, but the minimal fuser filesystem
  still failed before `lookup/readdir` with the same early `Connection closed`.
  This rules out init flag advertisement as a sufficient fix and reinforces
  that the macOS path should move to a libfuse high-level adapter unless a new
  lower-level trace identifies a precise fuser request/reply encoding defect.
- Continue source-level comparison with two minimal probes. Hosted run
  `25376060153` enabled fuser default callback logging and found no hidden
  unimplemented callback before disconnect; fuser only reported a short read
  after FUSE-T closed the connection. Hosted run `25376436969` proved a
  C/libfuse low-level hello filesystem succeeds on the same FUSE-T NFS hosted
  runner. The next fuser patch therefore targets the mount-channel difference:
  macOS now uses current `fuse_mount()` / `fuse_chan_fd()` to preserve FUSE-T's
  channel monitor/callback behavior, while Linux remains on
  `fuse_mount_compat25()`.
- Record hosted run `25376658909`: the first current-`fuse_mount()` patch
  failed at compile time because fuser moves mount state into its background
  session thread and raw `*mut c_void` channel storage made `MountImpl` fail
  Rust's `Send` bound. The follow-up patch keeps the same libfuse channel
  hypothesis but stores the macOS `fuse_chan` handle as an opaque integer and
  casts it back only for `fuse_unmount()`, allowing the minimal fuser hello
  runtime test to proceed.
- Record hosted run `25376827613`: patched fuser now compiles and mounts
  through FUSE-T, but still closes after the initial `statfs/getattr` sequence.
  Comparing fuser with `macos-fuse-t/libfuse` identified the next concrete
  mismatch: FUSE-T uses a stream socket and libfuse reads exactly one framed
  request by first reading `fuse_in_header` and then the remaining
  `header.len` bytes, while fuser still assumed Linux `/dev/fuse` packet
  semantics from a single large `read()`. The next patch makes macOS receive
  one framed FUSE-T request per session-loop iteration and leaves Linux
  unchanged.
- Record expanded comparison logs from hosted runs `25377362323` and
  `25377364111`: the successful C/libfuse low-level path starts with
  `proto=7.23 max_write=33554432` and then reaches `lookup/open/read`; patched
  fuser negotiates `ABI 7.19 max_write=16777216` and is closed after two
  `statfs/getattr` pairs. The next single-variable fuser patch aligns macOS
  `MAX_WRITE_SIZE` with FUSE-T's Darwin 32 MiB user/kernel buffer while leaving
  Linux at fuser's upstream 16 MiB.
- Record hosted run `25377547576`: the 32 MiB `max_write` alignment changed
  FUSE-T negotiation to `max_write=33554432` but did not change the failure
  shape. The next source-level difference is init reply payload size: FUSE-T's
  request minor is 23, which made fuser send its newer FUSE3-sized
  `fuse_init_out`, while FUSE-T's bundled libfuse2 success path still replies
  with the Darwin 24-byte init payload. The next patch keeps macOS init replies
  at `FUSE_COMPAT_22_INIT_OUT_SIZE` regardless of incoming FUSE-T minor version
  and leaves non-macOS behavior unchanged.
- Record hosted run `25377712709`: the Darwin 24-byte init reply patch fixed
  the minimal fuser hello probe and let FUSE-T proceed into lookup/open/read.
  The first full Operon smoke after that, run `25377799125`, then failed later
  at `mv` with `Input/output error`, proving the session handshake was fixed
  and the next issue was a rename-path incompatibility.
- Record hosted run `25378047581`: failure diagnostics showed fuser decoded a
  normal macOS FUSE-T rename as `name="" newname="renamed.txt"`. The root
  cause was fuser enabling the MacFUSE 4 `fuse_rename_in` request layout on a
  FUSE-T 1.2.1 `libfuse2-compatible` session. FUSE-T sends the legacy 8-byte
  payload, so the 16-byte MacFUSE layout skipped the old filename.
- Detect FUSE-T's pkg-config `-lfuse-t` mapping in the vendored fuser build
  script and keep the legacy libfuse2 `FUSE_RENAME` request ABI for FUSE-T
  while preserving MacFUSE 4 compatibility for non-FUSE-T macOS libfuse2
  builds.
- Validate macOS live mount on GitHub-hosted `macos-latest` for commit
  `c045b0a` in run `25378255568`; the smoke covered seed exposure, read,
  write, truncate, mkdir, rename, delete, remote read-back, and cleanup through
  the FUSE-T NFS adapter.
- Check the libfuse3 alternative with hosted run `25379933991`: forcing
  fuser's macOS mount implementation to `libfuse3` made FUSE-T negotiate
  `profile=v3 client=libfuse3`, but the minimal fuser hello probe still closed
  after root `statfs/getattr` and never reached `lookup/open/read`. v0.14
  should keep the already validated FUSE-T `libfuse2-compatible` path instead
  of switching macOS live mount to fuser's libfuse3 mount implementation.
- Remove the temporary libfuse3 force switch from the v0.14 workflow, macOS
  fuser hello probe, and vendored fuser build script before merging the
  investigation branch. The mainline implementation now selects the validated
  FUSE-T `libfuse2-compatible` path on macOS and keeps libfuse3 only as a
  documented failed alternative, not as a supported runtime toggle.

Remaining:

- Publish and verify a release only after live smoke and release artifact
  validation pass.

## Planning Principle

Every phase should preserve the core boundary:

```text
Cloudflare Mesh / Tailscale / WireGuard / SSH / LAN solve connectivity.
Operon solves what connected machines are allowed to do, how execution is
composed, and how results are traced.
```
