# Optional embedded connectivity

Status: Accepted for implementation (Phases 154–159); not yet released.

Existing private-network `grpc://`/`grpcs://` addressing remains the default.
The user approved an additional, explicit opt-in embedded iroh connection
backend. This supersedes the blanket exclusion of all embedded connectivity,
not the exclusions of an Operon VPN, IP/device mesh, custom NAT protocol,
global routing, subnet routing or a managed relay infrastructure product.
Use upstream iroh for node connections; never launch an external networking
process on behalf of a client or daemon. Self-hosted upstream relays are a
separate operator-managed deployment, not an implicit Operon-owned service.

The connection boundary carries the existing gRPC protocol as HTTP/2 on QUIC
streams, with native server incoming/client connectors. Do not ship the
experimental TCP bridge. Logical node IDs remain independent of cryptographic
endpoint IDs. Embedded peer authentication does not grant capabilities or
replace bearer authentication, policy, audit or workspace containment.
Relay use and discovery/publication are explicit: no automatic enrollment in
public infrastructure and no silent fallback to a different backend.

Keep protocol/networking implementations out of `operon-core`. Introduce an
owned transport crate so future backends can implement the same connection
boundary. Endpoint and admission tasks must have bounded concurrency and
explicit shutdown/join ownership. Connection and handshake waits and all
operational timeouts are configurable; disabling a deadline must not disable
shutdown cancellation. Persistent private identities require the existing
Unix private permissions/Windows ACL guarantees, including restart and
rotation acceptance.

The implementation intentionally upgrades workspace MSRV to Rust 1.91 and
pins release/development builders to 1.91.0. Validate both MSRV and latest
stable. Preserve GNU/musl/Alpine/OpenRC/macOS/Windows and the public TypeScript
SDK, with embedded SDK behavior tested in-process, not via a Rust CLI proxy.

Production readiness requires real self-hosted Docker relay/path evidence,
hosted CI for those network scenarios, native platform tests, full old/new
transport regression, README setup/security/timeout/support guidance, GitHub
Security alert review and exact-source plus public-package release gates.
See [the roadmap](../plan/v0.18.26-optional-embedded-connectivity.md).
