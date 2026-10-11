# iroh gRPC PoC (Phase 153)

Experimental, isolated PoC for optional embedded connectivity: unmodified tonic
gRPC carried over an authenticated iroh QUIC bidirectional stream. It changes
nothing in the production workspace, config, defaults, or release.

## Scope

- `src/lib.rs` — `QuicIo` (AsyncRead/AsyncWrite + tonic `Connected` exposing the
  transport-authenticated `EndpointId`), a `tower` connector channel, and a
  local-only `Endpoint` preset (no relays, no discovery publication, no TCP
  proxying).
- `tests/transport.rs` — native tonic server/client directly over iroh streams.
- `tests/operon.rs` — the unchanged real `operond` reached through a PoC-only
  in-process TCP loopback bridge (iroh stream ↔ `TcpStream`). This is NOT
  production native incoming; it proves the daemon needs no protocol change.
- `poc.proto` plus the production `proto/operon/runtime.proto` are both
  compiled, so the daemon test speaks the real runtime protocol.
- `examples/iroh_server.rs` / `examples/iroh_client.rs` — container-mode
  binaries. The server binds all interfaces with IP transports kept, prints its
  endpoint id and observed direct addresses, and serves the shared `Echo`
  probe service; the client dials by node id plus explicit `IP:port` entries
  (`EndpointAddr::from_parts` + `TransportAddr::Ip`) with no relay and no
  discovery lookup. `network_endpoint()` in `src/lib.rs` builds both.
- `Dockerfile` + `docker-run-cross-container.sh` — cross-container verification
  (see below).

## How to run

Requires Rust 1.91 (`rustup toolchain install 1.91`), protoc >= 3.15 for proto3
optional (system protoc 3.12.4 is too old; a downloaded protoc 29.3 lives in
`../.tools/bin/protoc`), and a built `operond`:

```sh
PROTOC=../.tools/bin/protoc \
OPERON_POC_DAEMON=<path-to-operond> \
cargo +1.91 test -- --nocapture
```

## Measured results (2026-10-11, WSL2, debug build, 1.91.1, iroh 1.3.0, tonic 0.14.5)

transport.rs — native tonic HTTP/2 over an authenticated iroh stream:

- PASS wrong ALPN rejected
- PASS unary >8MiB exact payload round-trip with peer identity extension
- PASS 64 concurrent verified 4KiB calls; reference ops/s ≈ 5.0–5.4k (debug,
  local-only, not a performance claim)
- PASS interactive bidirectional streaming and half-close (server replies
  before client EOF)
- PASS RPC deadline cancels the server handler
- PASS caller abort without an RPC deadline cancels the server handler
- PASS owned endpoint close terminates deadline-zero RPCs
- PASS owned accept/server tasks joined cleanly

operon.rs — unchanged real daemon over the PoC loopback bridge:

- PASS real `health` with bearer auth preserved over iroh; missing bearer
  rejected `UNAUTHENTICATED`
- PASS real fs streamed write/read >8MiB, >2MiB `ReadFileRange`, path escape
  rejected `PERMISSION_DENIED`
- PASS real exec, completion event stream, binary log content
- PASS direct TCP and iroh paths coexist against the same daemon
- PASS owned bridge tasks joined; owned daemon cleaned by RAII

## Cross-container verification (Docker, 2026-10-11)

Two separate containers on an isolated Docker bridge network (distinct network
namespaces, real IP path — not loopback):

```sh
docker build --build-context proto=<repo>/proto -t operon-iroh-grpc-poc .
./docker-run-cross-container.sh
```

Server container on `172.20.0.2:49145`, client dialed its node id plus that
explicit address with no relay and no discovery:

- PASS cross-container unary >8MiB exact payload
- PASS cross-container 64 concurrent verified calls
- PASS cross-container interactive bidirectional streaming

This is still not cross-NAT evidence: Docker bridge networking has no NAT
traversal or firewall asymmetry, and the client used pre-shared addresses
rather than discovery.

Fixes made while finishing the PoC:

- `operond` arg order: `start --config <path>` (config flag belongs to the
  `start` subcommand).
- The bridge tolerates connection-copy errors only after explicitly owned
  teardown begins; unexpected pre-shutdown errors and task panics propagate.
- Container resources use unique names and exact created IDs; a conflict never
  authorizes deleting an existing container. Test timeout is configurable through
  `OPERON_POC_TEST_TIMEOUT_SECS` (default 60). The server handles SIGINT, bounds
  pending accepts and joins its accept tasks; the harness checks clean exit.
- Build and runtime use the same Debian bookworm baseline. Docker builds use
  Rust 1.91.0 and `--locked`. Experimental `target/` is excluded from git.

The `Embedded Connectivity` workflow runs actual native gRPC/real-daemon tests
and the distinct-container check on hosted CI. Its initial wiring is not a
passed online result; actual run evidence must be recorded after commit/push.
Self-hosted relay and cross-NAT gates are required in the production roadmap
`docs/plan/v0.18.26-optional-embedded-connectivity.md`, not claimed by this PoC.

Closeout revalidation (2026-10-11): Rust 1.91.0 in an isolated Alpine build
container passed all three integration tests, all-target Clippy with `-D warnings`
and rustfmt. A fresh Rust 1.91.0/bookworm locked release image passed the
distinct-container >8MiB unary, 64 concurrent calls, interactive duplex and
SIGINT/accept-task join checks. Direct address was `172.20.0.2:37170`; this is
explicit-address local network evidence only. The harness removed only its
created client/server containers and network, retaining cidfiles for diagnostics.

## Remaining production gates (explicitly out of PoC scope)

- Cross-NAT traversal and relay failover evidence: native unit tests are loopback
  and container examples are explicit-address direct paths; no public relay or discovery
  publication was used.
- Production native incoming: the daemon path here is an explicit experimental
  bridge; a real integration needs daemon-owned iroh accept loops.
- Release-build performance, ops/s acceptance, and memory bounds per the
  `docs/quality/parallel-read.md` methodology.
- macOS/Windows verification, MSRV/lockfile policy for moving iroh out of
  `experiments/`, and SDK/docs/skills surfaces.
- Authorization mapping: PoC trusts the ALPN-checked peer `EndpointId`;
  production needs token/policy interplay with embedded identity.
