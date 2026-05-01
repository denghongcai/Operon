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

If the daemon is started with `--auth-token` or `--auth-token-file`, send bearer
metadata on every call:

```text
authorization: Bearer <token>
```

Missing or invalid metadata returns gRPC `Unauthenticated`.

## Service

All methods are on `operon.runtime.v1.OperonRuntime`.

Unary calls:

- `Health`
- `GetNode`
- `ListCapabilities`
- `StatFs`
- `ListFs`
- `WriteFileRange`
- `TruncateFs`
- `MkdirFs`
- `DeleteFs`
- `RenameFs`
- `CopyFs`
- `RunJob`
- `GetJob`
- `ListJobs`
- `CloseJobStdin`
- `CancelJob`
- `ListServices`
- `CheckService`
- `ListAudit`

Server-streaming calls:

- `ReadFile`
- `StreamJobLogs`

Client-streaming calls:

- `WriteFile`
- `WriteJobStdin`

## Streaming Rules

`ReadFile` returns ordered `FileChunk` messages. Concatenate `data` bytes in
receive order.

`WriteFile` accepts ordered `WriteFileRequest` messages. The first message must
set `path`. Later messages may leave `path` empty. A stream cannot switch paths.
It replaces the file content.

`WriteFileRange` writes one byte range at `offset`. It is intended for OS mount
adapters and other clients that need write-through random write behavior.

`TruncateFs`, `MkdirFs`, `DeleteFs`, and `RenameFs` are unary filesystem
mutation calls used by the Linux mount adapter. `CopyFs` is a daemon-side,
same-node file copy convenience operation for CLI, SDK, and direct protocol
clients. The daemon still owns policy and audit for these operations.

The human CLI maps these mutation calls to:

```text
operon fs mkdir <node:/path>
operon fs truncate <node:/path> --size <bytes>
operon fs rm <node:/path>
operon fs rename <node:/from> <node:/to>
operon fs copy <node:/from> <node:/to>
```

`WriteFileRange` is intentionally not exposed as a normal human CLI command. It
is a low-level protocol operation for mount adapters and direct clients.

`CopyFs` requires read permission on `from_path` and write permission on
`to_path`. It is same-node only. Cross-node copy is a separate future protocol
decision because it needs source streaming, target writing, failure recovery, and
audit ownership across two nodes.

## Filesystem Concurrency

The current filesystem protocol does not provide conflict detection.

Filesystem mutation requests do not carry file versions, etags, lock tokens,
leases, or compare-and-swap preconditions. `ReadFile` is a live stream from the
remote daemon, not a snapshot read. `WriteFile` replaces file content, and
`WriteFileRange` writes the requested byte range, but neither operation checks
that the file is unchanged since a prior read.

If multiple clients mutate the same path concurrently, Operon does not define a
merge order beyond the order in which the remote daemon and underlying
filesystem apply operations. Clients that need deterministic behavior should
serialize mutations outside the protocol until a later versioning or lease
contract exists.

`StreamJobLogs` returns ordered `JobLog` messages. Each message has `stream`
(`stdout` or `stderr`) and text `data`.

`WriteJobStdin` accepts ordered `JobStdinRequest` messages. The first message
must set `job_id`. Later messages may leave `job_id` empty. A stream cannot
switch jobs. Use `CloseJobStdin` to close the target job's stdin.

## Job Semantics

`RunJob` requires `command`.

`cwd` may be empty; the daemon treats an empty cwd as its policy default.

`timeout_secs` is used only when `has_timeout_secs` is true. Leave
`has_timeout_secs` false to use daemon policy defaults.

`secrets` is a list of secret names requested for the process environment.
Policy decides which names are allowed.

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
  -d '{"path":"/"}' \
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
const metadata = Metadata().set("authorization", "Bearer docker-token");

const health = await client.health({}, { metadata });
console.log(health);

const chunks: Uint8Array[] = [];
for await (const chunk of client.readFile({ path: "/hello.txt" }, { metadata })) {
  chunks.push(chunk.data);
}
```

The `http://` channel target above is the h2c URI expected by `nice-grpc`; the
daemon is still serving the Operon gRPC protocol only.
