# Optional embedded iroh (source implementation in progress)

Not yet present in the published v0.16.14 binaries. Phases 154–159 require
additional relay, SDK, native/mount, regression and exact-source/public-release
acceptance before supported publication. Do not confuse the isolated TCP-bridge
PoC with the production path: the daemon now accepts native HTTP/2 on QUIC
streams, and the shared Rust connector carries the unchanged gRPC protocol.

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
ordinary private-network endpoints; additional embedded discovery/onboarding
UX remains in Phase 156.

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
timeout. Values are bounded at seven days; no fixed short application deadline
is required for international links. Daemon shutdown remains configurable with
`--shutdown-timeout-secs` (zero waits indefinitely).

## Relay configuration and remaining acceptance

Relays are opt-in operator configuration, not an Operon-managed network.
`relay_urls` enables the local endpoint's relay transport; client
`peer_relay_urls` explicitly describes the peer's relay address. For relay-only
targets configure both, and omit `peer_addrs`. HTTPS is required by default;
HTTP needs explicit `allow_insecure_relays: true` and is intended only for
isolated tests. URLs must not embed credentials, a query or fragment.

The first self-hosted Docker relay gate passed locally with both endpoints'
IP transports disabled: pinned identities and >8MiB data matched, QUIC paths
reported relay, and the relay's sent/received counters exceeded 8MiB. Run
`docker build -t operon-iroh-relay-acceptance docker/iroh-relay` followed by
`bash scripts/verify-embedded-relay.sh`; the script publishes random loopback-only
ports and removes only its own container. HTTP is test-only, not a production
relay recommendation. Online execution is required for the committed source.

Distinct-container relay/NAT topology, denied-direct/UDP,
direct upgrade, relay loss/recovery, public SDK in-process connectivity and
complete platform/live-mount regression remain Phase 157–159 gates. Do not
claim those scenarios passed from the current direct-path tests. The current
public TypeScript SDK still uses ordinary grpc/grpcs addressing; its embedded
implementation is not yet complete.
