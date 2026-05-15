# musl / Alpine Distribution Decision

Status: Accepted for v0.18.7.

## Decision

Decision: keep glibc-only public Linux archives for now.

Operon will continue publishing the current GNU/glibc Linux release archives
and will document Alpine and musl-based distributions as unsupported by the
prebuilt Linux archives. A separate musl/static artifact line is not planned
until there is concrete user demand, a release blocker, or a maintainer-owned
follow-up phase with artifact, CI, and support scope.

## Current Evidence

- v0.18.5 verifies the glibc release baseline by running the downloaded public
  archive on `ubuntu:20.04`, which represents glibc 2.31, and on `debian:12`,
  which represents a current stable glibc distribution.
- `scripts/assess-musl-alpine-distribution.sh` records the expected
  unsupported behavior for the same public glibc archive on `alpine:3.20`.
- Alpine uses musl libc by default. The current public Linux archives are
  linked for GNU/glibc and should not be presented as Alpine-compatible
  binaries.
- A future `x86_64-unknown-linux-musl` release target would need a dedicated
  artifact decision, dependency audit, mount-runtime assessment, CI smoke, and
  README/release verifier updates before publication.

## Options Considered

1. Keep glibc-only archives.
   - Pros: smallest release matrix, current CI already proves a real glibc
     baseline, fewer platform-specific support branches.
   - Cons: Alpine users must build from source, use a glibc-based environment,
     or wait for a future musl/static artifact phase.

2. Add musl/static Linux archives.
   - Pros: potentially broader Linux binary portability and clearer Alpine
     install path.
   - Cons: new Rust target and dependency surface, possible larger binaries,
     extra artifact names, checksum and release verification changes, and
     separate mount-runtime behavior to support.

3. Publish Alpine package or container images.
   - Pros: native Alpine installation experience.
   - Cons: introduces package or image publishing operations that are outside
     the current archive-based release model.

4. Leave Alpine behavior undocumented.
   - Rejected. Users should get a direct support answer rather than a generic
     loader failure.

## Policy

- Supported prebuilt Linux archives: glibc-based distributions compatible with
  the documented glibc baseline, currently validated with `ubuntu:20.04` and
  `debian:12`.
- Unsupported prebuilt Linux archive targets: Alpine and other musl-based
  distributions.
- Supported workaround for Alpine users: build Operon from source in the
  target environment or run the prebuilt binary in a glibc-based environment.
- Follow-up trigger: create a new phase for musl/static artifacts only when the
  owner is ready to add artifact naming, CI build/smoke, checksum validation,
  release docs, and install-usability workflow coverage together.

## Validation

Use dry-run mode when editing docs or validation wiring:

```bash
scripts/assess-musl-alpine-distribution.sh --dry-run v0.16.6 denghongcai/Operon
```

Use the full assessment when a container runtime is available:

```bash
OPERON_CONTAINER_RUNTIME=podman scripts/assess-musl-alpine-distribution.sh v0.16.6 denghongcai/Operon
```
