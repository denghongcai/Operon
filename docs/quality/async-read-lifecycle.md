# Async read lifecycle implementation checkpoints

Status: In Progress (Phases 148–152). These checkpoints are not final release
acceptance. Preserve the failed Phase 147 prototype evidence separately.

## Cancellation and automatic external-unmount detection

Linux mount shutdown first closes range-read admission and broadcasts
cancellation to active read and other mount RPC waits. This also applies when
`rpc_timeout_secs: 0`; shutdown does not rely on an RPC timeout. FUSE receivers
have a shared level-triggered stop channel and are joined, not detached when
unmount returns an error. Async owned tasks are cancelled and joined during
filesystem destruction after workers finish.

The foreground mount process polls kernel `/proc/self/mountinfo`, not remote
filesystem metadata, and stops automatically when the original mount disappears
or is replaced. Session completion also ends the wait. Original numeric mount
identity is captured before the handshake; filesystem type/source are also checked
because numeric mount IDs may be recycled after destruction. Retaining an
`O_PATH` reference was tested and rejected: it makes external non-lazy unmount
return EBUSY. This reference is only temporary during identity capture.

The Linux detection interval defaults to 250ms and can be changed via:

```yaml
client:
  mount:
    unmount_poll_ms: 500
```

CLI override: `operon mount local:/ --to /mnt/remote --unmount-poll-ms 100`.
Valid range is 10–60000ms. This local detection interval is separate from
network RPC deadlines; international latency does not require shortening RPC
timeouts. Remote disk I/O already executing on the daemon is not guaranteed to
stop merely because the client RPC future was cancelled.

## Checkpoints on 2026-10-10

### Accepted concurrent-mount support boundary

The user explicitly accepted the following disclosure on 2026-10-10. Ordinary
external detach, subsequent replacement and automatic read cancellation remain
required and have passed Linux sync/async live tests. The mount identity check
is not atomic with the subsequent path-based unmount syscall/helper. A different
process with mount privileges can cover that path between those operations;
Operon cannot guarantee preserving that concurrently overlaid mount. Do not
concurrently cover an Operon mount while its owner is shutting down. Coordinate
mount changes or wait for the owner to exit before mounting a replacement.

Pinned-FD/cwd and move_mount alternatives were tested and rejected rather than
claimed as fixes: the former can follow overmounts, and the latter moves the
whole original subtree including its overmounts. Failed diagnostics are retained
in `evidence/async-read-unmount-identity-2026-10-10.json`. This accepted limitation
does not waive native platform, performance or exact-source release acceptance.

- Explicit-stop async probe: `/tmp/operon-lifecycle-async-fixed-proof` passed
  offset/EOF/zero reads, write/rename/truncate/delete, timeout recovery, normal
  shutdown, and external lazy unmount with four pending reads and deadline zero.
  In-flight shutdown was 0.00427s; both joins reported zero active RPCs.
- Broader replacement probes failed and were retained, revealing the need to
  capture identity before the handshake and account for mount-ID reuse.
- Automatic no-signal async probe:
  `/tmp/operon-lifecycle-auto-final-async-proof` passed the same read/mutation/
  timeout checks, external lazy unmount with four deadline-zero reads (0.03269s,
  ECONNABORTED for all four), zero post-join RPCs and a preserved replacement
  tmpfs mount. Expanded normal external-unmount and synchronous probes are
  separate required gates, not inferred from this result.
- Expanded async probe:
  `/tmp/operon-lifecycle-auto-final-async-expanded-proof` passed ordinary external
  unmount without a stop signal, all four task joins and replacement protection.
  Synchronous saturation probe still fails: one 4096-byte read completed after
  approximately six seconds. Measure helper-versus-actual-detach timing before
  attributing this to cancellation; do not suppress the assertion or claim
  synchronous acceptance.
  The follow-up `/tmp/operon-lifecycle-auto-sync-traced` measured the lazy
  unmount helper itself taking 5.803s while the read completed at 6.003s after
  submission (the helper was started after the 0.2s probe delay). This localizes
  the remaining investigation to pre-detach helper/control-request starvation,
  rather than proving a post-detach uncancelled RPC. A strict saturated kernel
  detach probe and helper behavior acceptance remain required.
- The strict direct-kernel control
  `/tmp/operon-lifecycle-sync-kernel-detach` passed four saturated synchronous
  deadline-zero reads, automatic cancellation/exit, ordinary external unmount
  and replacement preservation. It uses `umount2(MNT_DETACH | UMOUNT_NOFOLLOW)`
  on the disposable client's verified exact mount; no helper assertions were
  removed. This separates actual detach cancellation from helper latency. The
  failing original helper case remains a required investigation, not a passed
  gate inferred from this control.
- Helper syscall trace (`/tmp/operon-lifecycle-sync-helper-syscalls`) shows
  `umount2` itself taking 0.000501s, with no multi-second syscall inside the
  traced helper. Investigate pre-exec inherited-FUSE-descriptor closure and
  tracer startup effects; the traced pass-fds control still failed (one read
  completed at 3.002s), so descriptor closure is not yet a proven complete
  explanation. Keep original strict cancellation assertions.
- Untraced helper with explicit descriptor inheritance
  (`/tmp/operon-lifecycle-sync-helper-inherited-no-tracer`) passed the unchanged
  four-saturated-read cancellation assertion, automatic no-signal shutdown,
  ordinary external unmount and replacement preservation. The parent test
  owns open FUSE files; closing their inherited copies before helper exec is
  an additional FUSE operation, unlike invoking an external helper from a
  separate shell. The default probe now preserves those descriptors through
  helper exec. Strace instrumentation remains intrusive and its failed result
  is retained, not classified as a passing release gate. Repeat both default
  modes on final production binaries before closing Phase 149.
- Default mount tests passed 40; feature-enabled mount tests passed 42; CLI
  tests passed 58; config tests passed 11. Shared receiver stop test passed in
  an isolated copy of the vendored crate. Targeted strict clippy passed before
  the subsequent identity refinements and must be rerun on final source.
- Vendored crate tests: 53 passed on the host; its real mount test failed because
  the host lacks a mount runtime, then passed unchanged in the isolated FUSE
  container. No test was skipped to reinterpret the host failure as success.

Remaining release gates include repeating no-signal shutdown and foreground
interrupt checks on final source, same-path replacement races (including another
Operon mount), saturation, Drop/error cleanup, native platforms, final ops-first
measurements, full suites, exact-source and public release verification.

## Formal scheduler and interrupt checkpoints

The experimental feature was removed. Source modes/configuration are described
in `docs/quality/async-read-configuration.md`; no public release has changed yet.
All passed and failed checkpoint results, source-state disclosures and binary
hashes are retained in `evidence/async-read-lifecycle-2026-10-10.json`.
No-feature formal binary `/tmp/operon-formal-read-checkpoint.ePpbOE` passed
explicit sync/async probes (`/tmp/operon-formal-read-sync-probes` and
`/tmp/operon-formal-read-async-probes`) before per-request interrupt routing.
CLI/config/core/mount tests passed 124 at that checkpoint after rerunning socket
tests outside the restricted sandbox.

The interrupt checkpoint `/tmp/operon-read-interrupt-checkpoint.elR5SV` then
passed both foreground aligned O_DIRECT nonfatal-signal probes:
`/tmp/operon-read-interrupt-direct-async-probes` and
`/tmp/operon-read-interrupt-direct-sync-probes`. Async EINTR returned in 0.00272s;
read-after-interrupt recovery, normal shutdown, external lazy/ordinary unmount
without SIGINT and replacement protection passed. Sync additionally asserted
zero active RPCs after all five session joins. The original buffered-read
interrupt tests timed out and remain retained; they are not counted as passed.

Current source mount unit tests passed 48 and strict workspace/all-targets
clippy passed after request-ID reuse, target-only interruption, admission
cancellation and per-mount activity tests. These are development checkpoints,
not exact-release-SHA gates. Drop/error cleanup, replacement race hardening,
native platforms, saturation/disconnection and final performance/release
acceptance remain required.

## Session Drop and busy-descriptor checkpoint

Session destruction and explicit shutdown share one take-once ownership path.
It broadcasts cancellation before attempting unmount/join. If mount identity
cannot be verified, it disconnects and joins without unmounting an unknown path
and reports the verification error (Drop logs it).

The added busy-descriptor test first failed EBUSY with exit 1 at
`/tmp/operon-read-busy-handle-before-fix`. This failure remains archived. Linux
native unmount now uses lazy detach, matching the existing non-root helper,
so a user-held file descriptor does not prevent owned session shutdown.

Expanded probes on `/tmp/operon-read-busy-fixed-checkpoint.8mq2Ig` passed both
sync and async (`/tmp/operon-read-drop-busy-fixed-sync` and
`/tmp/operon-read-drop-busy-fixed-async`). Besides all earlier probes, they test
normal SIGINT with an open descriptor and invoke actual Rust session Drop and
unmount while four reads are in flight with deadlines disabled. All four reads
return errors; both modes finish with zero active RPCs. The compiled Linux
`session-lifecycle` helper checks `/proc/self/task` and `/proc/self/fd` after
return and rejects surviving fuser threads or `/dev/fuse` descriptors. This
helper is an optional test artifact, not a published user command.

Passed and failed results, source-state disclosures and binary/helper hashes
are retained in `evidence/async-read-drop-2026-10-10.json`. Mount tests (48) and
strict workspace/all-targets clippy passed at this source checkpoint. Same-path
replacement race hardening, full saturation/disconnection, native platforms,
final ops measurements and exact-SHA/public release verification are still open.
