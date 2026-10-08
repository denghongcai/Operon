# musl / Alpine Distribution Decision

Status: Additional musl artifacts implemented under release acceptance; current
public v0.16.11 remains glibc-only until Phase 141 publishes verified v0.16.12.

## Approved Follow-up

The user approved Phases 137–141 in
`docs/plan/v0.18.22-musl-alpine-distribution-roadmap.md`: add fully static
x86_64/arm64 musl archives alongside GNU/glibc, with native Alpine runtime/FUSE/OpenRC
and complete verified release coverage. This supersedes the earlier decision
not to plan musl builds, not the current released support boundary. Existing
glibc archives remain unsupported on Alpine until new artifacts are published
and verified. Native x86_64/arm64 Alpine 3.22 and 3.23 runtime/FUSE/OpenRC
acceptance passed on `82ea08506f78e7b6d7ac33f5cd7dd288c314fe71` in run
`37800708634`; CI `37800708675` and CodeQL `37800708019` passed. Source
acceptance does not substitute for public downloaded-package verification.

apk, ARMv7 musl and Cloudsmith are excluded. OpenRC service management is now
approved as a dedicated phase, preserving current systemd behavior and requiring
real supervision/lifecycle acceptance on both native Alpine architectures.
Current released Linux `operond service` remains systemd-based until that phase
is implemented and published; fake-systemd tests do not establish OpenRC support.

## Decision

Decision: add fully static musl archives alongside existing GNU/glibc archives.

The historical decision was to keep glibc-only public Linux archives for now.
It is superseded for v0.16.12 and newer by the approved additional artifact line:
`operon-<tag>-linux-musl-x86_64.tar.gz` and
`operon-<tag>-linux-musl-arm64.tar.gz`. GNU names and their glibc 2.31 minimum
remain unchanged; GNU archives themselves remain unsupported on Alpine.
New releases must contain ten assets, while historical releases retain their
eight-asset contract. This is not yet a claim of published Alpine support.

## Current Evidence

- v0.18.5 verifies the glibc release baseline by running the downloaded public
  archive on `ubuntu:20.04`, which represents glibc 2.31, and on `debian:12`,
  which represents a current stable glibc distribution.
- `scripts/assess-musl-alpine-distribution.sh` records the expected
  unsupported behavior for the same public glibc archive on `alpine:3.20`.
- Alpine uses musl libc by default. The current public Linux archives are
  linked for GNU/glibc and should not be presented as Alpine-compatible
  binaries.
- `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` now have pinned
  native builds, fully static ELF checks and real runtime/FUSE/OpenRC acceptance.
  Draft release uses the same native acceptance workflow; the public release
  still requires downloaded install/runtime/live mount and service evidence.

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
- Intended additional musl baseline: native x86_64/arm64 Alpine 3.22 and 3.23,
  after v0.16.12 publication and downloaded verification. No blanket support
  claim covers historical Alpine, ARMv7 musl or every other musl-based OS.
- Supported workaround for Alpine users: build Operon from source in the
  target environment or run the prebuilt binary in a glibc-based environment.
- Distribution selection: `scripts/lib/release-assets.sh` detects libc, not
  just architecture or installed development loaders. Unknown/ambiguous hosts
  require `OPERON_RELEASE_LIBC=gnu|musl`; unsupported architectures do not
  silently fall back. OpenRC system scope requires an explicit existing non-root
  service account; systemd user service behavior remains unchanged.

## Validation

Use dry-run mode when editing docs or validation wiring:

```bash
scripts/assess-musl-alpine-distribution.sh --dry-run v0.16.7 denghongcai/Operon
```

The following assessment is specifically the negative GNU-on-Alpine test, not
the musl support gate. Use it when a container runtime is available:

```bash
OPERON_CONTAINER_RUNTIME=podman scripts/assess-musl-alpine-distribution.sh v0.16.7 denghongcai/Operon
```

Positive public verification is `scripts/verify-alpine-release.sh <tag> <repo>`
through the native `Verify Alpine Release` workflow. APK packages and Cloudsmith
remain outside scope. See `docs/quality/alpine-openrc.md` for service scope,
private files, logs, configurable waits and foreground fallback.
