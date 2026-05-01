# Runtime API

Operon exposes daemon runtime operations through gRPC only. The API is meant for
the `operon` CLI, SDKs, generated protocol clients, and agent tooling on an
already trusted private network. HTTPS, mTLS, and signed node identity remain
later hardening work.

## Current Boundary

- The daemon starts one runtime listener with `operond start --grpc-listen`.
- Runtime configs use `grpc://` or `grpcs://` node endpoints.
- Scripts should use `operon --json`, not direct daemon calls.
- Programs should use an SDK or generate a gRPC client from
  `proto/operon/runtime.proto`.
- Clients that do not use an SDK should follow `PROTOCOL.md`.
- Authentication is bearer-token based when `operond` starts with
  `--auth-token` or `--auth-token-file`.
- Policy is enforced by the daemon for every capability operation.
- gRPC errors use status codes for auth, policy, validation, missing resources,
  precondition failures, and internal failures.
- Long-running execution remains job-based with explicit status calls or
  streaming log/stdin methods.

There is no HTTP runtime facade. A gRPC client library may internally use an
`http://` h2c channel target for `grpc://` endpoints, but that is transport
configuration, not an Operon HTTP API.

The runtime API is not a VPN, proxy, relay, or port forwarder. Network
encryption, routing, and private addressing should come from Cloudflare Mesh,
Tailscale, WireGuard, SSH tunnels, LAN, Kubernetes networking, or another
existing secure network layer.

## gRPC Runtime Surface

The source of truth is `proto/operon/runtime.proto`.

Unary calls:

- `Health`
- `GetNode`
- `ListCapabilities`
- `StatFs`
- `ListFs`
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

## Service / Port Capability

The service capability describes local TCP services that daemon policy allows
callers to inspect.

Policy example:

```yaml
service:
  services:
    - id: daemon
      name: daemon
      host: 127.0.0.1
      port: 7789
      protocol: tcp
      description: Operon gRPC daemon listener
```

`ListServices` returns configured services. `CheckService` attempts a TCP
connection to one configured service and records an audit event. Unknown service
ids fail through policy.

This capability does not forward traffic, proxy bytes, allocate ports, or create
network reachability. It only exposes configured metadata and health checks.

## Interface Policy

`operon` is the supported human, ops, and script interface. Use `--json` when a
script needs stable machine-readable output.

SDKs and generated clients are the supported programmatic interface. A new
daemon runtime surface must be added to `proto/operon/runtime.proto` first, then
exposed through CLI/SDK as needed.
