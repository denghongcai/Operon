# Bounded asynchronous FUSE read experiment — 2026-10-10

Phase 147 is completed as a **measurement experiment**, not production async
mount acceptance. Public v0.16.13, versions and defaults remain unchanged.
Normal builds still use synchronous FUSE reads. The Linux-only
`operon-mount/experimental-async-read` build feature delays the FUSE reply until
an asynchronous range RPC completes, using the same two-thread Tokio runtime.
No connection pool, new content/fd cache, larger kernel range or kernel queue
tuning was introduced. Per-RPC daemon authorization/resolution/audit remains.

## Method and executed binaries

Source baseline: `9196519052e1292247040cf1ddbf74404899ff60` (the v0.16.13
release source plus evidence-only documentation). Both variants were built
locally with Rust 1.88.0 release mode; these are not comparisons of downloaded
GitHub artifacts. The daemon binary is identical in every variant.

```text
sync operon     8b8e78ebb049bfdbb6134159023a356ea2c2015ec2ee32ecf9a23c889746dc8f
async operon    0646550453dba15e9cd2a9db1d8c6acc46cdb17ff4a51aadab87f58f9690dd8a
both operond    2d3f33a567228212415377d4e87590754b9478f153551880879a01b168fda51d
```

The x86_64 host reported 24 available CPUs and no cgroup CPU quota. Main
comparisons fix **four FUSE workers, two Tokio workers and 32 MiB requested-byte
budget**. Baseline read concurrency is eight; async candidates use 8/16/32.
A separate control compares 16 synchronous workers against four async workers,
both with 16 RPC permits. Async queued/active tasks are separately capped at 64;
at capacity the prototype uses synchronous backpressure rather than unbounded
spawn or returning artificial EAGAIN. This fallback has not passed saturation
and live cancellation acceptance.

Seven matrices use three alternating paired trials, 1/4/8/16 readers, verified
16 MiB complete-file reads and verified offset-sensitive 4 KiB random preads.
Each case remounts the client; daemon OS page cache is intentionally warm.
Same-file readers and random workloads may share client page cache. Completed
file/pread operations are the ops/s unit, not batches or RPCs. Local random
uses 100 operations per reader, delayed random uses 32. Delayed tests inject
20 ms **client egress** delay only inside the disposable container; this is not
a measured real international link or a fixed RTT guarantee.

All three trials, bytes, audited RPC counts, network counters, reader elapsed
times, mount RSS, cgroup resources and shutdown high-water logs are pinned in
[machine-readable evidence](evidence/async-read-2026-10-10.json). A short
unit-test invocation overlapped startup of the first local-16-RPC trial;
the later two trials and other matrices were isolated from tests. This caveat
and all trials are retained, not removed to improve a result. Host variation
means absolute numbers from separate matrices must not be mixed as a matched
comparison. Ratios below are ratios of paired-matrix medians, not guaranteed
speedups or latency percentiles.

## Distinct-file throughput (ops/s primary)

| Location / readers | Sync 4 workers, 8 RPC | Async 4 workers, 16 RPC | Ratio |
| --- | ---: | ---: | ---: |
| Local / 1 | 11.40 | 12.03 | 1.06x |
| Local / 4 | 20.28 | 30.19 | 1.49x |
| Local / 8 | 19.42 | 36.86 | 1.90x |
| Local / 16 | 19.27 | 42.52 | 2.21x |
| 20 ms egress / 1 | 0.645 | 0.644 | 1.00x |
| 20 ms egress / 4 | 1.350 | 2.440 | 1.81x |
| 20 ms egress / 8 | 1.380 | 3.549 | 2.57x |
| 20 ms egress / 16 | 1.344 | 4.646 | 3.46x |

With identical four-worker/eight-RPC settings, asynchronous scheduling alone
improved distinct-file four/eight/sixteen-reader local ops/s by 41%/62%/54%,
and delayed ops/s by 81%/93%/91%. It lets RPC concurrency exceed callback worker
count; it does not make remote I/O intrinsically multiple times faster.

At the same 16-RPC limit, increasing the synchronous worker count also recovers
most of the throughput. The independent control measured:

| Local readers | Sync 16 workers | Async 4 workers | Ratio |
| --- | ---: | ---: | ---: |
| 1 | 13.25 | 13.50 | 1.019x |
| 4 | 31.94 | 33.38 | 1.045x |
| 8 | 41.06 | 43.70 | 1.064x |
| 16 | 45.71 | 48.64 | 1.064x |

This control supports **similar or modestly better throughput with fewer
blocked OS threads**, not a universal async speedup. It was local only;
no matched-16-worker delayed control was run.

## RPC limits, regressions and resources

Measured async RPC peaks were 8/16/19 for the respective 8/16/32 limits, in both
local and delayed matrices. Every normal teardown log recorded
`active_after_join=0`. A 32-RPC setting did **not** produce 32 active RPCs;
kernel/application dispatch still matters. Sixteen RPCs captured most of this
workload's gain; 32 did not show a consistent additional benefit, and there is
no evidence here for an automatic CPU-count-derived default.

Regressions are explicit: local same-file eight-reader ops/s fell 6.3% in the
8-RPC comparison; delayed same-file four-reader fell 9.0% with eight RPCs,
and same-file eight-reader fell 9.3% with 16 RPCs. Delayed low-concurrency
random reads were broadly unchanged or slightly slower (16-RPC four-reader
random ratio 0.972x). These cache-sharing workloads are not interchangeable
with distinct files. All results, including other regressions and variability,
remain in the JSON rather than only selected positive rows.

Local 16-RPC comparison maxima: mount RSS 12,196 KiB baseline vs 14,608 KiB
candidate, daemon cgroup peak 19,218,432 vs 29,605,888 bytes. The matched control
recorded mount maxima 17,956 KiB with 16 synchronous workers vs 14,736 KiB with
four async workers. These are observed maxima over the matrix, not hard memory
limits. Cgroup memory includes charged page cache; one delayed-8-RPC baseline
trial peaked at 298,057,728 bytes. That outlier is retained and cannot justify
a general async memory-reduction claim. The read budget bounds active requested
RPC bytes, not total RSS, completed reply buffers or kernel caches.

## Correctness and lifecycle findings

Default workspace: 277 Rust tests passed; feature workspace: 279 passed,
including task capacity, slot reuse and abort/join of indefinitely pending
futures. Strict all-target feature workspace clippy, format/diff and Python
syntax checks passed. The opt-in resolver benchmark remained ignored (one);
this experiment does not claim a new resolver benchmark or SDK/native-platform
release gate. All 504 throughput cases verified payloads and normal teardown.

Independent live probes passed in **both binaries**:

- Offset 37 data, EOF and zero-size reads.
- Write-through, rename, truncate and delete, checked against daemon backing files.
- Configured one-second queue/RPC deadlines under three-second egress delay;
  reads failed and then recovered after delay removal, proving budgets released.
- Normal unmount. Async normal teardown logged zero active RPCs after join.

Observed whole `pread` duration was about two seconds in both binaries despite
the one-second RPC limit: an RPC/queue deadline is not an end-to-end kernel
syscall deadline. The test retains these measured durations rather than claiming
every OS read finishes within one second.

**The in-flight external lazy-unmount probe FAILED in both binaries.** With
deadlines disabled and open descriptors, `fusermount3 -uz` followed by CLI
SIGINT causes exit 1 / `EINVAL` from the second unmount. The existing session
path returns before a reliable background-thread/task join; no second async
zero-active log was observed. Initial probe cleanup also encountered ENOTCONN
while closing detached descriptors; its original log is retained, and the
revised probe preserves the primary EINVAL plus preceding successful checkpoints.
Both failed probe exit statuses remain failures, not successful acceptance.

Raw failures: `/tmp/operon-async-read-lifecycle`,
`/tmp/operon-async-read-lifecycle-v2`, `/tmp/operon-sync-read-lifecycle`.
Unit-level abort/join success does not replace this failed live lifecycle gate.
Before productizing async reads, define and validate externally-unmounted/open
handle shutdown behavior, saturation cancellation, per-request FUSE interrupt
handling and native adapter support. This experiment did not modify the
published unmount behavior or claim those follow-ups were complete.

## Reproduction

Save the unchanged release binaries before applying the prototype:

```sh
cargo +1.88.0 build --release --locked -p operon-cli -p operond
# Copy operon/operond to an owned baseline directory.
cargo +1.88.0 build --release --locked -p operon-cli -p operond \
  --features operon-mount/experimental-async-read
# Copy operon/operond to a separate owned candidate directory.
python3 scripts/performance/parallel-read.py \
  --baseline /path/to/sync --candidate /path/to/async \
  --mib 16 --readers 1 4 8 16 --operations 100 --repeats 3 \
  --baseline-mount-args '["--mount-workers","4","--read-concurrency","8","--read-budget-mib","32"]' \
  --mount-args --mount-workers 4 --read-concurrency 16 --read-budget-mib 32
```

For delayed cases add `--delay-ms 20` and use `--operations 32`, before
`--mount-args`. Change candidate concurrency to 8/32 for the other matrices;
the matched control also changes baseline workers/concurrency to 16.
`--acceptance-probes` runs independent lifecycle checks outside timing and is
**expected to fail the known external-unmount case**, not an all-green gate.
Requires Docker, `/dev/fuse`, the existing review-tools image, and iproute2
(installed only in disposable containers if absent). No host cache flush or
global network setting is changed. Owned containers/networks are cleaned up;
raw output is retained, and successful throughput fixtures are gzip-compressed.

Raw timing evidence: `/tmp/operon-async-read-local-c8`, `local-c16`, `local-c32`,
`delay20-c8`, `delay20-c16`, `delay20-c32`, `local-matched16` (all latter names
have the same `/tmp/operon-async-read-` prefix). No new tag/release was created.
