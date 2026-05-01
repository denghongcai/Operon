# Runtime API

Operon v0.5 uses gRPC as the core runtime protocol and keeps HTTP/JSON as a
scriptable compatibility facade. The API is meant for local CLI, SDK, and agent
tooling on an already trusted private network. HTTPS, mTLS, and signed node
identity remain later hardening work.

## Current Boundary

- The daemon listens on HTTP when `--listen` is set.
- The daemon listens on gRPC when `--grpc-listen` is set.
- Authentication is bearer-token based when `operond` starts with
  `--auth-token` or `--auth-token-file`.
- Policy is enforced by the daemon for every capability operation.
- HTTP errors use one structured JSON response shape.
- gRPC errors use status codes that map to the same authz and policy outcomes.
- Long-running execution remains job-based with explicit polling or streaming
  log/stdin endpoints.

The HTTP API is not a VPN, proxy, relay, or port forwarder. Network encryption,
routing, and private addressing should come from Cloudflare Mesh, Tailscale,
WireGuard, SSH tunnels, LAN, Kubernetes networking, or another existing secure
network layer.

## Error Response

All daemon handlers should return this JSON shape on structured errors:

```json
{
  "code": "forbidden",
  "message": "fs read denied by policy",
  "status": 403,
  "capability": "fs:workspace",
  "resource": "/secret.txt"
}
```

Fields:

- `code`: stable machine-readable class such as `unauthorized`, `forbidden`,
  `not-found`, `bad-request`, `timeout`, or `internal-error`.
- `message`: human-readable detail.
- `status`: HTTP status code.
- `capability`: optional capability id involved in the failure.
- `resource`: optional path, service id, job id, or other target resource.

## Stable HTTP Facade Surfaces

Node and capability:

- `GET /health`
- `GET /node`
- `GET /capabilities`

Filesystem:

- `GET /fs/stat?path=...`
- `GET /fs/list?path=...`
- `GET /fs/read?path=...`
- `POST /fs/write`
- `GET /fs/read-stream?path=...`
- `POST /fs/write-stream?path=...`

Jobs:

- `POST /job/run`
- `POST /job/cancel`
- `GET /job/status?id=...`
- `GET /job/list`
- `GET /job/logs?id=...`
- `GET /job/logs-stream?id=...`
- `POST /job/stdin?id=...`
- `POST /job/stdin/close?id=...`

Audit:

- `GET /audit`

Services:

- `GET /service/list`
- `GET /service/check?id=...`

## Service / Port Capability

The service capability describes local TCP services that the daemon policy
allows callers to inspect.

Policy example:

```yaml
service:
  services:
    - id: daemon
      name: daemon
      host: 127.0.0.1
      port: 7788
      protocol: tcp
      description: Operon daemon TCP listener
```

`/service/list` returns configured services. `/service/check` attempts a TCP
connection to one configured service and records an audit event. Unknown service
ids fail through policy.

This capability does not forward traffic, proxy bytes, allocate ports, or create
network reachability. It only exposes configured metadata and health checks.

## Stable gRPC Runtime Surface

The source of truth is `proto/operon/runtime.proto`. v0.5 exposes:

- unary calls for health, node metadata, capabilities, fs stat/list, job
  run/status/list/cancel, services, and audit.
- server-streaming calls for file reads and job logs.
- client-streaming calls for file writes and job stdin.

The HTTP API remains the compatibility and local-control surface for scripts and
debugging.
