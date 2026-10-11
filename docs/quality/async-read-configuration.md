# Formal FUSE read configuration (source checkpoint, not yet published)

Phases 148–152 are in progress. Public v0.16.13 remains unchanged; its historical
benchmark results are not final evidence for the following source defaults.

```yaml
client:
  mount:
    mode: auto                 # auto, sync, async
    worker_threads: 4          # Linux only; omit for platform default
    max_pending_reads: 64      # 1–4096 queued + active owned FUSE tasks
    max_inflight_reads: 8      # 1–64 active range RPCs
    max_inflight_read_mib: 32  # 8–512 requested MiB, not RSS
    unmount_poll_ms: 250       # 10–60000, Linux external-detach detection
```

```sh
operon --rpc-timeout-secs 300 mount remote:/ --to /mnt/remote \
  --read-mode async --mount-workers 4 --read-pending 64 \
  --read-concurrency 16 --read-budget-mib 32 --unmount-poll-ms 250
```

`auto` selects asynchronous FUSE reads on Linux, synchronous dispatch on macOS
and the existing native WinFsp dispatcher on Windows. Explicit `async` is implemented for Unix FUSE; native macOS
acceptance is still required. Windows rejects it instead of silently changing
WinFsp dispatch. Explicit `sync` uses the callback's thread for the read RPC;
it does not create a per-read OS thread. Final defaults require native and
ops-first acceptance before publication.

Task admission, RPC/byte-budget queues and the read RPC share one deadline
captured when the read callback begins. Admission does not reset it. Saturated
admission waits are cancellable; there is no fallback synchronous RPC. At most
`max_pending_reads` asynchronous tasks are owned; additional admission waits
are bounded by the callback-worker count and remain in kernel backpressure.
No statement here bounds time before the kernel dispatches a callback or
guarantees a process-RSS cap. `rpc_timeout_secs: 0` disables network deadlines,
not session shutdown cancellation.

Keepalive detects a stale HTTP/2 connection; it is not an overall deadline for
requests waiting on channel reconnection. With RPC deadlines disabled, a
blackholed link can leave reads waiting for connectivity indefinitely. Configure
a finite `rpc_timeout_secs` (large enough for the intended international link)
to bound this wait. Explicit and detected external unmount still cancel it.

Paths are normalized before either mode dispatches. Daemon authorization,
workspace resolution and audit remain per request. No content cache,
speculative read, persistent remote descriptor, schema change or kernel tuning
is introduced. Existing write-through and offset/EOF behavior is preserved.

Per-read FUSE interrupt routing now targets only a registered request ID and
replies EINTR from its original reply owner. Unknown IDs use kernel EAGAIN
retry handling; completed registrations are removed and reused numeric IDs
cannot be removed by an older guard. Request cancellation is not a guarantee
that daemon disk I/O already running has stopped. Linux buffered readahead may
not generate an interrupt for a nonfatal signal; real foreground O_DIRECT
interrupt validation is a separate gate. Callback-admission saturation can
also delay delivery of kernel control requests; session shutdown uses a
separate cancellation path rather than waiting for their delivery.

RPC activity counters are scoped to each remote mount client, so closing one
mount does not misreport another mount's in-flight reads. Scheduler destruction
cancels and joins owned tasks before its runtime can be released.

Windows retains native WinFsp dispatch rather than adopting the Unix task
scheduler. Owned shutdown broadcasts remote cancellation before stopping the
WinFsp dispatcher, so callback completion does not depend on a nonzero deadline.
Native Windows acceptance remains required before publication.

Mount identity is checked before owned path-based unmount, but the check and
syscall/helper are not atomic. Another mount-privileged process can cover the
path in that interval. This explicitly accepted concurrency limitation does not
affect tested ordinary external detach followed by replacement. Coordinate
mount changes and wait for the original owner to exit before replacing it;
do not concurrently overmount a path during its owner's shutdown.
