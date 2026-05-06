# Release and CI Observability

Status: Updated for v0.17.

This note records where release-critical checks run and how to handle
deterministic failures. It does not replace the per-phase acceptance plans; it
is the operator-facing map for finding the right log quickly.

## Default CI

- `CI / Validation` runs `scripts/ci/run-validations.sh` by capability group.
  Add new version validation scripts to the narrowest existing group unless the
  check needs a different OS, permission model, service container, or trigger.
- `CI / TypeScript` owns `pnpm -r test`. Validation scripts that only need to
  prove CI wiring should support `OPERON_SKIP_SDK_TESTS=1` so the same script
  remains locally runnable without duplicating SDK tests in CI.
- `CodeQL` is the security analysis gate for release commits. Treat a red
  CodeQL run as release-blocking until the alert or tooling failure is
  understood.

## Release Gates

- `Cross-Platform Live Mount Smoke` is the live mount release gate. Run it
  manually before tagging with `platform=all`; macOS artifacts include the
  uploaded `macos-live-mount-<backend>.log` file and Windows logs live on the
  `Windows WinFsp Live Mount` job.
- `Draft Release` runs on tag push. Its `Release Gates` job checks that live
  mount smoke passed for the tag commit before artifacts are built.
- `Verify Release Artifacts` is manual and downloads public assets on Linux,
  macOS, and Windows. It validates the complete asset set, SHA256SUMS, archive
  layout, binary smoke, and SDK tarball contents.
- `Verify README Quickstart` is manual and runs the public README install flow
  in Docker against the provided tag.

## Failure Triage

- Pull the failing job log as soon as the failed step identifies the error; do
  not wait for unrelated matrix jobs when the failure is deterministic.
- Cancel obsolete workflow runs after a targeted fix is pushed, especially
  release-gate and live-mount runs that are tied to an older commit.
- Rerun only the workflow that proves the fix: default CI for code/script
  changes, live mount smoke for mount-runtime changes, release draft for tag
  packaging, public artifact verification for published assets, and README
  Quickstart for install-flow changes.
