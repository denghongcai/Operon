# Operon Runtime Protocol

Operon daemon runtime operations are exposed through gRPC only. The supported
human and script interface is `operon`, including `operon --json`. Programs can
use the TypeScript SDK, or they can integrate directly with the protocol in this
document without using an Operon SDK.

There is no HTTP runtime API.

## Source Of Truth

The protocol definition lives at:

```text
proto/operon/runtime.proto
```

Generate a client for your language from that file and call:

```text
operon.runtime.v1.OperonRuntime
```

## Endpoint Schemes

Operon node configs use:

```yaml
nodes:
  local:
    endpoint: grpc://127.0.0.1:7789
    token: optional-token
```

`grpc://host:port` means cleartext gRPC over HTTP/2, often called h2c. Many
gRPC client libraries still expect the channel target to be written as
`http://host:port` for this mode. That `http://` target is only a gRPC transport
URI, not an Operon HTTP API.

`grpcs://host:port` is reserved for TLS gRPC endpoints. Full TLS identity and
mTLS policy are separate roadmap items.

## Authentication

If the daemon config sets `daemon.auth.token`, `daemon.auth.token_file`, or
`daemon.auth.token_env`, send bearer metadata on every call:

```text
authorization: Bearer <token>
```

Missing or invalid metadata returns gRPC `Unauthenticated`.

## Policy Decision Vocabulary

The daemon owns capability authorization. Filesystem, job, service, and secret
checks use a shared policy decision vocabulary internally and in audit reasons.
Denied policy decisions include a stable reason code before the human-readable
message, for example:

```text
job-cwd-denied: job cwd denied by policy
service-action-denied: service `web` action `forward` denied by policy
```

Current reason codes include `fs-mount-not-allowed`,
`fs-permission-denied`, `job-cwd-denied`, `job-timeout-exceeded`,
`secret-denied`, `secret-undefined`, `service-unknown`,
`service-action-denied`, and `unsupported-action`. Existing audit filters still
filter by subject, capability, action, resource, and allowed state; the reason
field remains a string for compatibility.

`operon config explain --json` exposes the effective policy grants under
`policy.effective_grants`. Each grant includes `capability_id`, `action`,
`resource`, `allowed`, and `reason_code`. Secret values are never exposed.

Use the unary `ExplainCapability` RPC to ask the daemon why one capability
action is allowed or denied before attempting the operation. The request carries
`capability_id`, `action`, `resource`, and optional `timeout_secs` for job
diagnostics. The response is a `PolicyDecision` with `subject`, `capability_id`,
`action`, `resource`, `allowed`, `reason_code`, and `message`.

## Execution Context Metadata

Clients that execute an Operon graph should attach optional context metadata to
capability calls:

```text
x-operon-run-id: <run id>
x-operon-step-id: <step id>
```

The daemon copies these values into audit events. Direct clients that do not
run graphs can omit them.

## Schema Conventions

Runtime enum fields are protobuf enums on the wire:

- `Capability.kind`: `CAPABILITY_KIND_FS`, `CAPABILITY_KIND_PROCESS`,
  `CAPABILITY_KIND_JOB`, `CAPABILITY_KIND_DEVICE_INFO`,
  `CAPABILITY_KIND_SERVICE`.
- `JobRecord.status` and `JobEvent.status`: `JOB_STATUS_RUNNING`,
  `JOB_STATUS_SUCCEEDED`, `JOB_STATUS_FAILED`, `JOB_STATUS_CANCELLED`,
  `JOB_STATUS_TIMED_OUT`.
- `ServiceDefinition.protocol`: `SERVICE_PROTOCOL_TCP`.

Fields whose absence is meaningful use proto3 `optional`, not paired `has_*`
booleans. This applies to job timeout, job exit code, service check reason, and
audit run/step context. Generated clients should leave the optional field unset
when the value is absent.

List request messages use `page_size` and `page_token`. A `page_size` of `0`
means "return all remaining items" for low-level protocol clients. Non-zero
page sizes are capped by the daemon. Responses include `next_page_token`; an
empty token means there are no more pages. The `operon` CLI and high-level SDK
helpers walk pages internally to preserve complete-list behavior.

## Service

All methods are on `operon.runtime.v1.OperonRuntime`.

Unary calls:

- `Health`
- `GetNode`
- `ListCapabilities`
- `ExplainCapability`
- `StatFs`
- `ListFs`
- `ReadFileRange`
- `WriteFileRange`
- `TruncateFs`
- `MkdirFs`
- `DeleteFs`
- `RenameFs`
- `CopyFs`
- `RunJob`
- `GetJob`
- `ListJobs`
- `ListJobLogs`
- `CloseJobStdin`
- `CancelJob`
- `ListServices`
- `CheckService`
- `ListAudit`

Server-streaming calls:

- `ReadFile`
- `WatchJob`
- `StreamJobLogs`

Client-streaming calls:

- `WriteFile`
- `WriteJobStdin`

Bidirectional-streaming calls:

- `OpenServiceTunnel`
- `OpenServiceDatagramTunnel`

UDP/datagram forwarding uses datagram-specific envelopes rather than reuse of
the TCP byte-stream tunnel.

## Streaming Rules

`ReadFile` returns ordered `FileChunk` messages. Concatenate `data` bytes in
receive order.

`ReadFileRange` reads one byte range from `path` at `offset` with `size` bytes
and returns a single `FileChunk`. It is the efficient random-read API for OS
mount adapters and direct clients that need seek-style reads. `ReadFile`
remains the streaming full-file API.

`WriteFile` accepts ordered `WriteFileRequest` messages. The first message must
set the `target` variant with `WriteFileTarget.path`. Later messages must set
the `chunk` variant with `FileChunk.data`. A stream cannot send duplicate
targets or switch paths. It replaces the file content. An empty file write is
encoded as target metadata followed by a single empty `FileChunk.data`; clients
should not treat that as an omitted write.

`WriteFileRange` writes one byte range at `offset`. It is intended for OS mount
adapters and other clients that need write-through random write behavior. The
daemon rejects oversized chunks, offset/data overflow, and writes beyond its
maximum fs object size bound. `ReadFileRange` applies the same offset/size bound
checks before reading.

`TruncateFs`, `MkdirFs`, `DeleteFs`, and `RenameFs` are unary filesystem
mutation calls used by the Linux mount adapter. `CopyFs` is a daemon-side,
same-node file copy convenience operation for CLI, SDK, and direct protocol
clients. `MkdirFs` creates missing parent directories. The daemon still owns
policy and audit for these operations.

The human CLI maps these mutation calls to:

```text
operon fs mkdir <node:/path>
operon fs truncate <node:/path> --size <bytes>
operon fs rm <node:/path>
operon fs rename <node:/from> <node:/to>
operon fs copy <node:/from> <node:/to>
```

`ReadFileRange` and `WriteFileRange` are intentionally not exposed as normal
human CLI commands. They are low-level protocol operations for mount adapters
and direct clients.

`CopyFs` requires read permission on `from_path` and write permission on
`to_path`. It is same-node only. Cross-node copy is a separate future protocol
decision because it needs source streaming, target writing, failure recovery, and
audit ownership across two nodes.

## Filesystem Concurrency

The current filesystem protocol does not provide conflict detection.

Filesystem mutation requests do not carry file versions, etags, lock tokens,
leases, or compare-and-swap preconditions. `ReadFile` is a live stream from the
remote daemon, not a snapshot read. `ReadFileRange` reads the requested bytes at
call time. `WriteFile` replaces file content, and `WriteFileRange` writes the
requested byte range, but neither operation checks that the file is unchanged
since a prior read.

The daemon resolves filesystem targets under the configured workspace and
rejects symlink-resolved paths that escape that workspace.

`DeleteFs` and `RenameFs` use leaf-symlink semantics: when the requested path
itself is a symlink inside the workspace, the operation applies to the symlink
entry, not the symlink target. Parent directories are still resolved and checked
for workspace containment.

If multiple clients mutate the same path concurrently, Operon does not define a
merge order beyond the order in which the remote daemon and underlying
filesystem apply operations. Clients that need deterministic behavior should
serialize mutations outside the protocol until a later versioning or lease
contract exists.

Current containment checks are path-based and do not yet use Linux
`openat2(RESOLVE_BENEATH)`. Deployments should not allow untrusted local
processes to concurrently mutate the daemon workspace until fd-relative
resolution is added.

`WatchJob` returns ordered `JobEvent` messages for the requested job. The first
event is the current job state. Later events are status changes, including the
terminal state. Clients that need to wait for a job should prefer `WatchJob`
over polling `GetJob`.

`ListJobLogs` returns the daemon's current in-memory log ring for a job.
`StreamJobLogs` returns `JobLogStreamEvent` envelope messages and stays open
while the job is running. The stream can carry:

- `snapshot`: the current retained log window plus `truncated`,
  `dropped_log_count`, and `next_sequence`.
- `entry`: a single ordered stdout/stderr `JobLog` chunk.
- `complete`: terminal job status plus final log truncation metadata.

Each `JobLog` has `stream` (`stdout` or `stderr`), binary `data`, and a
monotonic `sequence` number. Job logs are not embedded in `JobRecord`; they are
stored as separate append-only log records and retained in a bounded in-memory
ring. `JobRecord` only reports `log_count` and `logs_truncated`. Clients should
decode `data` only at presentation boundaries; the protocol preserves
stdout/stderr bytes, including non-UTF-8 output.

`WriteJobStdin` accepts ordered `JobStdinRequest` messages. The first message
must set the `target` variant with `JobStdinTarget.job_id`. Later messages must
set the `chunk` variant with `FileChunk.data`. A stream cannot send duplicate
targets or switch jobs. Use `CloseJobStdin` to close the target job's stdin.

## Service Forwarding

`ListServices` returns services explicitly configured in daemon policy,
including per-action service permissions. `CheckService` requires
`permissions.check`, attempts a TCP or UDP check against one configured service,
and records an audit event. UDP health is connection-setup-only: a successful
UDP socket connect is reported with `udp socket connected; datagram response
not verified`, because UDP does not prove application reachability without a
protocol-specific request/response.

`OpenServiceTunnel` is the low-level protocol for explicit local port
forwarding. The first client message must set `ServiceTunnelRequest.target`
with the policy service id. Later client messages set `data` to send bytes to
that service or `close` to half-close the client side. The server first returns
`opened` after connecting to the configured service, then returns `data` chunks
from the service, and finally returns `close` when the remote service closes or
the tunnel fails. TCP forwarding requires `permissions.forward`.

The daemon only connects to `host:port` values present in its own
`policy.service.services`. It does not accept arbitrary destination host/port
pairs from clients.

The human CLI maps this to:

```text
operon service list <node-id>
operon service check <node-id> <service-id>
operon service forward <node-id> <service-id> --listen 127.0.0.1:8080
operon service forward-udp <node-id> <service-id> --listen 127.0.0.1:5353
```

Forwarding is local and explicit: the CLI binds the requested local listener,
and each accepted TCP connection opens one gRPC service tunnel to the selected
node. Operon still relies on an existing private network for reachability
between the client and daemon. It does not provide VPN behavior, NAT traversal,
relay networking, mesh IP assignment, global routing, or unmanaged port
exposure.

`OpenServiceDatagramTunnel` is the low-level protocol for UDP forwarding. The
first client message must set `ServiceDatagramTunnelRequest.target` with the
policy service id. Later client messages set `datagram` with a stable `peer_id`
and packet bytes. The server returns datagrams with the same `peer_id`, allowing
the client-side forwarder to route each response to the original local UDP peer.
The server can send `close` for one peer or for the whole tunnel. A peer close
has `peer_id`; a whole-tunnel close uses an empty `peer_id`. UDP forwarding also
requires `permissions.forward`.

Datagram forwarding preserves packet boundaries. It uses idle peer-session
cleanup and daemon-side packet size checks. It is still local and explicit: the
CLI binds a local UDP socket, and the daemon sends only to the configured UDP
service. It does not provide UDP hole punching, mDNS relay behavior, QUIC
transport replacement, relay networking, or arbitrary host/port forwarding.

## Job Semantics

`RunJob` accepts either `command` or `argv`.

`command` is the compatibility path. It is a shell command string executed by
the daemon with `/bin/sh -c`. Use it when shell operators, expansion,
redirection, or pipelines are required.

`argv` is the shell-free path. The first item is the executable and later items
are passed as process arguments without shell parsing. Prefer `argv` for agent
and SDK calls when arguments are already structured. Do not set both `command`
and `argv`; clients should choose one execution contract per request.

`cwd` may be empty; the daemon treats an empty cwd as its policy default.

Set optional `timeout_secs` to request a per-job timeout. Leave it unset to use
daemon policy defaults.

`secrets` is a list of secret names requested for the process environment.
Policy decides which names are allowed.

The daemon clears the inherited process environment before spawning jobs. It
then injects only variables named by `job.env_allowlist` from the daemon
environment and authorized requested secrets.

Set `policy.job.preserve_env: true` to preserve the daemon's complete
environment for spawned jobs. This is useful when commands need normal process
context such as `HOME`, `PATH`, proxy variables, or toolchain variables, but it
also grants jobs access to every environment variable visible to `operond`.

## Errors

Clients should handle normal gRPC status codes:

- `Unauthenticated`: missing or invalid bearer token.
- `PermissionDenied`: policy denied a capability operation.
- `NotFound`: path, job, service, or other resource was not found.
- `InvalidArgument`: malformed request, invalid stream ordering, or missing
  required fields.
- `FailedPrecondition`: operation is valid but the current runtime state cannot
  accept it.
- `Internal`: daemon-side failure.

The status message is intended to be human-readable. Scripts should branch on
the gRPC status code, not on message text.

## grpcurl Examples

`grpcurl` can call unary methods without an SDK:

```bash
grpcurl -plaintext \
  -H "authorization: Bearer docker-token" \
  -import-path proto \
  -proto operon/runtime.proto \
  127.0.0.1:17790 \
  operon.runtime.v1.OperonRuntime/Health
```

List files:

```bash
grpcurl -plaintext \
  -H "authorization: Bearer docker-token" \
  -import-path proto \
  -proto operon/runtime.proto \
  -d '{"path":"/","page_size":100,"page_token":""}' \
  127.0.0.1:17790 \
  operon.runtime.v1.OperonRuntime/ListFs
```

Run a job:

```bash
grpcurl -plaintext \
  -H "authorization: Bearer docker-token" \
  -import-path proto \
  -proto operon/runtime.proto \
  -d '{"command":"echo hello","cwd":"/","secrets":[]}' \
  127.0.0.1:17790 \
  operon.runtime.v1.OperonRuntime/RunJob
```

Run a shell-free argv job:

```bash
grpcurl -plaintext \
  -H "authorization: Bearer docker-token" \
  -import-path proto \
  -proto operon/runtime.proto \
  -d '{"argv":["printf","hello world"],"cwd":"/","secrets":[]}' \
  127.0.0.1:17790 \
  operon.runtime.v1.OperonRuntime/RunJob
```

For streaming methods, use a generated gRPC client and send or receive the
message sequences described above.

## Direct TypeScript Example

This uses generated protocol bindings and `nice-grpc` directly, without
`@operon/sdk`.

```ts
import { createChannel, createClient, Metadata } from "nice-grpc";
import { OperonRuntimeDefinition } from "./generated/operon/runtime";

const channel = createChannel("http://127.0.0.1:17790");
const client = createClient(OperonRuntimeDefinition, channel);
const metadata = Metadata()
  .set("authorization", "Bearer docker-token")
  .set("x-operon-run-id", "run-example")
  .set("x-operon-step-id", "step-read");

const health = await client.health({}, { metadata });
console.log(health);

const chunks: Uint8Array[] = [];
for await (const chunk of client.readFile({ path: "/hello.txt" }, { metadata })) {
  chunks.push(chunk.data);
}

let nextSequence = 0;
for await (const event of client.streamJobLogs({ jobId: "job-1" }, { metadata })) {
  if (event.snapshot) {
    for (const log of event.snapshot.logs) {
      if (Number(log.sequence) >= nextSequence) {
        nextSequence = Number(log.sequence) + 1;
        process.stdout.write(Buffer.from(log.data));
      }
    }
    nextSequence = Math.max(nextSequence, Number(event.snapshot.nextSequence));
  } else if (event.entry?.log && Number(event.entry.log.sequence) >= nextSequence) {
    nextSequence = Number(event.entry.log.sequence) + 1;
    process.stdout.write(Buffer.from(event.entry.log.data));
  }
}
```

The `http://` channel target above is the h2c URI expected by `nice-grpc`; the
daemon is still serving the Operon gRPC protocol only.
