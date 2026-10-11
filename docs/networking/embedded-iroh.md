# Embedded connectivity

Optional iroh connectivity runs inside Operon, without an external connectivity
process. It requires matching v0.16.15 daemon, client and SDK packages or source
builds. Download matching packages from the
[v0.16.15 release](https://github.com/denghongcai/Operon/releases/tag/v0.16.15).
Older packages do not support this option.

## Configuration contract

`grpc://` and `grpcs://` remain the default and continue to work over LAN, DNS,
Tailscale/WireGuard or other existing private networks. `iroh://<public-key>`
selects an additional connection backend; there is no silent backend fallback.
The cryptographic endpoint ID is not Operon's logical `node_id`.

Source builds need Rust 1.91 or newer; the repository pins 1.91.0. Distributed
daemon/CLI builds compile iroh support, but start no iroh endpoint without an
explicit configuration selection. A `--no-default-features` daemon/CLI build
can omit the backend; selecting it in such a build is an error.

Keep the normal daemon/client policy and auth sections. Add a listener block:

```yaml
daemon:
  # Existing node_id, grpc_listen, workspace, auth and policy stay unchanged.
  iroh:
    identity_file: iroh-server.key
    bind_addrs: ["0.0.0.0:7788"]
    max_connections: 128
    handshake_timeout_secs: 10
```

An embedded listener always requires a non-empty daemon bearer token, including
when its IP bind is loopback. Peer encryption/authentication does not grant any
capabilities. Use the normal `operond start --config <path>` or service manager.
Startup logs report the **public** endpoint ID and observed addresses. Private
keys are never printed. Share the public ID and normal auth credentials through
your trusted pairing workflow; do not copy the server's private key to clients.

For a client on the same private network, configure an explicit peer address:

```yaml
client:
  nodes:
    remote-logical-name:
      endpoint: iroh://REPLACE_WITH_SERVER_PUBLIC_ENDPOINT_ID
      auth:
        token_file: remote.token
      transport:
        connect_timeout_secs: 30
        rpc_timeout_secs: 60
        iroh:
          identity_file: iroh-client.key
          peer_addrs: ["192.168.1.20:7788"]
```

Then use unchanged Rust CLI commands such as `operon node ping
remote-logical-name`, filesystem, exec or mount commands. The configuration
loader keeps unknown-field warnings and resolves client identity paths against
the config directory. There is no implicit global lookup, address publication
or enrollment in public relays. Existing LAN discovery continues to locate
ordinary private-network endpoints. An embedded discovery record does not
provide the local identity, routes or bearer token needed to connect: pair it
explicitly with `operon onboard`, then use the configured node's `node ping`.
Discovery export rejects an unpaired embedded record before writing config;
exporting an already configured matching peer preserves its identity, routes
and authentication. Discovery health checks report when explicit pairing is
required.

Client onboarding accepts explicit identity and peer addresses:

```sh
operon onboard --role client --yes --output-dir .operon \
  --node-id remote --endpoint iroh://SERVER_PUBLIC_ENDPOINT_ID \
  --iroh-identity-file client.key --iroh-peer-addr 192.168.1.20:7788
```

Supply auth through the normal trusted token/config workflow. Repeat
`--iroh-peer-addr` for explicit addresses, or use repeatable
`--iroh-relay-url https://your-relay.example` for both local and peer relay
selection. HTTPS remains required; onboarding does not enable insecure relays,
public lookup or generate private identities. Identity initialization happens
only when the selected transport starts. These are client options: the daemon
listener is configured separately as above. Default onboarding stays grpc.

Daemon onboarding has separate listener options:

```sh
operon onboard --role daemon --yes --output-dir .operon \
  --daemon-iroh-identity-file server.key \
  --daemon-iroh-bind-addr 0.0.0.0:7788
```

Use `--daemon-iroh-relay-url https://your-relay.example` to add a relay;
addresses and relays are repeatable. At least one listener route is required.
Daemon onboarding writes its normal private token file, not the identity key;
the daemon creates its key only on startup. After startup, pair clients using
the logged public endpoint ID. Client and server must have separate keys.

## Identity and timeouts

`identity_file` is explicit and relative to the config directory unless absolute.
The parent directory must exist. On first initialization a random private
32-byte identity is written to a protected temporary file, flushed and published
without overwriting an existing file. Concurrent initializers use the winner's
identity. Restart loads it unchanged. Unix uses owner-only permissions and
no-follow opens; Windows uses a protected current-user/Administrators/SYSTEM ACL
before writing key bytes, and verifies the opened handle's ACL. Existing broad,
malformed or symlink files are rejected, not repaired/replaced automatically.
Private key buffers are zeroized. Rotation changes the paired public ID: keep
the previous key private and update pairing after restarting with the new key.

HTTP/2 RPC/transfer/progress/connect and keepalive settings stay under
`transport`. Under the iroh endpoint block, the additional configurable timers
are `bind_timeout_secs`, `readiness_timeout_secs`, `idle_timeout_secs`,
`keepalive_interval_secs`, `path_idle_timeout_secs` and
`path_keepalive_interval_secs`; the listener adds `handshake_timeout_secs`.
Connection/readiness/idle/handshake deadlines accept zero to disable the
deadline; closing the endpoint still cancels waits and admissions. QUIC/path
heartbeat intervals are positive and must be shorter than the applicable idle
timeout. The upstream iroh 1.3 per-path idle limit is 15 seconds: its default
is 15, and larger values are rejected rather than silently clamped. This is a
transport path-liveness constraint, not an application RPC timeout; it cannot
be enlarged through Operon configuration. Other timers are bounded at seven
days; no fixed short application deadline
is required for international links. Daemon shutdown remains configurable with
`--shutdown-timeout-secs` (zero waits indefinitely).

## Self-hosted relays

Relays are opt-in operator configuration, not an Operon-managed network.
`relay_urls` enables the local endpoint's relay transport; client
`peer_relay_urls` explicitly describes the peer's relay address. For relay-only
targets configure both, and omit `peer_addrs`. HTTPS is required by default;
HTTP needs explicit `allow_insecure_relays: true` and is intended only for
isolated tests. URLs must not embed credentials, a query or fragment.

For a private-CA HTTPS relay, set `relay_ca_file: relay-ca.pem` in each local
daemon/client iroh configuration (`relayCaFile` in the SDK). The file adds PEM
CA certificates to the existing public trust anchors; hostname, validity and
chain checks remain enabled. Rust config-relative paths resolve beside the
configuration file; SDK paths should be absolute. Empty, malformed or larger
than 1 MiB files fail explicitly, without falling back to unverified TLS.
This affects relay TLS, not the pinned Ed25519 peer identity or bearer policy.

## TypeScript SDK

The SDK uses an optional in-process native adapter for embedded connections.
Existing `OperonClient` methods stay unchanged; select `iroh://<public-key>` and
set `transport.iroh` with `identityFile`, explicit `peerAddrs` or `peerRelayUrls`,
and the matching local relay settings. SDK settings use camelCase. Identity
paths resolve from the current working directory; use absolute paths for SDK
applications. Default grpc/grpcs clients do not load the binding.

Install the extracted JS SDK's runtime dependencies with npm 11.6.1 or newer:
`npm install --omit=dev --ignore-scripts`. npm 10 can fail while resolving this
package's dependency graph; Node 22 remains supported with the newer installer.

Extract the matching native SDK archive and set `OPERON_SDK_NATIVE_PATH` to the
absolute path of `operon-sdk-native.node`, or place that file in the SDK's
`native/` directory. Match the SDK version, OS, CPU and libc; GNU and musl are
not interchangeable. Native bindings support Linux x86_64/arm64, macOS
x86_64/arm64 and Windows x86_64, not ARMv7. Missing or incompatible bindings
fail explicitly; there is no fallback to TCP.

For source builds, run `bash scripts/build-sdk-native.sh <rust-target>
<output-directory>` on the matching native platform and use the emitted
`operon-sdk-native.node`.

Select the embedded endpoint using the existing SDK API:

```ts
const client = new OperonClient([{
  nodeId: "remote",
  endpoint: "iroh://REPLACE_WITH_SERVER_PUBLIC_ENDPOINT_ID",
  token: tokenFromTrustedPairing,
  transport: {
    connectTimeoutSecs: 120,
    rpcTimeoutSecs: 300,
    iroh: {
      identityFile: "/private/iroh-client.key",
      peerAddrs: ["192.168.1.20:7788"],
    },
  },
}]);
try {
  await client.statFs("remote", "/");
} finally {
  client.close();
}
```

Interrupted operations return errors. Reconnect or resume explicitly where the
API supports it; Operon does not automatically replay mutations.

Small sequential requests measured slower than ordinary gRPC in source-build
benchmarks. See [performance results](../quality/embedded-connectivity-performance.md)
and the [release acceptance record](../plan/v0.16.15-release-publication.md).
