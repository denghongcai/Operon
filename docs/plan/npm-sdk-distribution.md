# npm SDK distribution (Phases 160–162)

Status: In progress. Publish the TypeScript SDK as
`@cellworks/operon-sdk-js` through `.github/workflows/npm-publish.yml`, GitHub
environment `npm`, hosted runners and npm OIDC. No long-lived npm token.

Current evidence: SDK typecheck/build and 42 unit tests pass. Workflow
`38268644346` completes packing, installation/import and real daemon/SDK native
business acceptance, including automatic platform selection. Its publish job
awaits the user's required `npm` environment approval. No publication is claimed
until registry integrity/provenance and installed package checks pass.
Both initial jobs now succeed after approval: npm confirms signed provenance
and accepted publication of 0.16.15. Registry processing/installation checks
remain in progress. Future workflow runs verify registry SHA512 equality and
installed API after a bounded availability wait. Historical v0.16.15 remains
17 assets; future releases are nine assets, with matching npm SDK acceptance.

## Phase 160 — Package and trusted publication

Keep the workspace's internal SDK name for existing development/CI filters;
generate a public manifest with the requested npm name, ESM/types exports,
repository, license and Node requirements. Publish compiled JS and declarations,
not source tests or build tools. Include seven matching native bindings in this
one package because only its trusted publisher is configured. They remain lazy
and optional at runtime; grpc does not load native code. Preserve explicit
`OPERON_SDK_NATIVE_PATH` overrides. Reject mismatched versions or platform files.

The initial version is 0.16.15 (the registry currently contains only a
placeholder). Reuse already accepted v0.16.15 native archives; prove build SHA,
successful native jobs, manifest version/target/hash and unchanged Rust sources.
Do not rebuild Rust merely to change distribution. Build/test the JS packaging
change, inspect npm tarball, install it privately and exercise actual bindings.
Publish with OIDC and verify registry integrity/provenance plus installed import.
The user's publisher repository field must match `denghongcai/Operon` exactly.

## Phase 161 — Future GitHub release asset contract

Keep all historical v0.16.15 assets unchanged. From v0.16.16 onward, GitHub
Releases contain eight daemon/CLI archives and SHA256SUMS only (nine assets).
SDK/native builds can remain CI artifacts feeding npm, never release uploads.
Update release validators and downloaded GNU/musl SDK gates to obtain the
matching version from npm; retain full business, relay and platform checks.
An absent npm version must fail the gate, not silently omit SDK acceptance.

## Phase 162 — Online acceptance and closeout

Require online OIDC publication before the configured validation deadline,
actual installed npm SDK/default/native capabilities, future-release asset
contract tests and existing affected SDK/release CI. Update concise usage and
phase records. Do not claim completion merely because a dry run succeeds.
