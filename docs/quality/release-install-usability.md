# Release Install Usability

Status: Added for v0.18.5.

This note records the post-download release verification path. It complements
`Verify Release Artifacts`: artifact verification proves the published asset
set and archive layout; install usability proves a downloaded archive can be
installed into an isolated prefix and used from `PATH`.

## Verify Release Install Usability

Run the manual `Verify Release Install Usability` GitHub Actions workflow after
a public release is published. It checks the provided tag on Ubuntu, macOS, and
Windows, then runs Linux container smoke on `ubuntu:20.04` and `debian:12`.

The workflow uses:

```bash
scripts/verify-release-install-usability.sh <tag> <owner/repo>
scripts/verify-release-service-management-smoke.sh <tag> <owner/repo>
scripts/verify-release-linux-install-containers.sh <tag> <owner/repo>
```

The first script downloads the current platform archive and `SHA256SUMS`,
verifies the archive checksum, installs `operon` and `operond` into a temporary
prefix, proves `PATH` resolves both commands from that prefix, runs version and
help checks, runs `operon doctor --mount-runtime`, starts a local foreground
daemon from the installed binary, and completes a minimal node/capability/fs
workflow.

The service-management smoke downloads the same public release archive into an
isolated prefix and runs `operond service install/start/status/stop/uninstall`
from that installed binary. It uses fake platform supervisor commands in CI so
the generated systemd unit, launchd plist, or Windows Service registration
arguments can be inspected without leaving persistent services installed on a
runner. On Windows, the smoke runs service commands from the fake supervisor
binary placed beside the installed `operond.exe` so `sc.exe` lookup is isolated
from the host Service Control Manager.

The Linux container wrapper is the release compatibility check for documented
glibc-based archives. It uses Docker on GitHub runners and can use Docker or
Podman locally. `ubuntu:20.04` represents the current glibc 2.31 minimum
baseline, while `debian:12` catches a current stable distribution path. Alpine
and musl-based distributions are unsupported by the prebuilt Linux archives.
The decision is recorded in `docs/decisions/musl-alpine-distribution.md`.

## Local Dry Run

Use dry-run mode while editing workflow or documentation wiring:

```bash
scripts/verify-release-install-usability.sh --dry-run v0.16.7 denghongcai/Operon
scripts/verify-release-service-management-smoke.sh --dry-run v0.16.7 denghongcai/Operon
scripts/verify-release-linux-install-containers.sh --dry-run v0.16.7 denghongcai/Operon
scripts/assess-musl-alpine-distribution.sh --dry-run v0.16.7 denghongcai/Operon
```

Dry-run mode does not download assets or start Docker. It validates argument
resolution, current-platform asset selection, Linux container image selection,
and the script entrypoints used by CI validation.

## Failure Triage

- If checksum download or verification fails, rerun `Verify Release Artifacts`
  first to determine whether the public release asset set is incomplete.
- If an installed command resolves outside the temporary prefix, inspect the
  workflow PATH setup before debugging Operon itself.
- If `operon doctor --mount-runtime` fails, treat it as a release usability
  issue even when live mount gates passed; downloaded binaries must surface
  mount runtime prerequisites clearly.
- If the local daemon smoke fails, inspect the uploaded workflow log around
  `operond.log`; this check exercises the installed product, not the source
  checkout.
- If service-management smoke fails, inspect the fake supervisor log in the
  workflow output first. It shows whether the installed `operond` binary
  generated the expected systemd, launchd, or Windows Service command.
- If Alpine or another musl-based host reports a loader-style failure, use a
  glibc-based Linux distribution or build from source. Alpine and musl-based
  distributions are unsupported by the prebuilt Linux archives.
