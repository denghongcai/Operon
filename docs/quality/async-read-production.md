# Production async read acceptance checkpoint

The implementation was subsequently published and fully verified as v0.16.14;
see [release gates](../plan/v0.16.14-release-publication.md). This does not
change the historical binary provenance or turn these into tagged-archive measurements.

These are Linux source-checkpoint measurements, not verification of a published
release. Both sides use the same Rust 1.88.0 production binary pair, explicitly
selecting sync versus async. Binary hashes, source HEAD/dirty status, Docker
image identity, all trials, network/RPC counters and resource records are in
[the retained evidence](evidence/async-read-production-2026-10-11.json).

Seven primary matrices contain 504 cases. Each has three alternating trials,
1/4/8/16 readers and same-file, distinct-file and random-offset workloads.
Sequential operations verify a complete 16 MiB file; random operations verify
4 KiB preads, 100 per reader. Kernel caching means application operations are
not interchangeable with RPC counts. There is no host cache flushing. The
delay setting is 20 ms client-egress netem in an isolated Docker network, not
a claim of a measured fixed 20 ms end-to-end RTT.

For 16 readers of distinct files, median application ops/s were:

| Matrix | Sync | Async | Async / sync |
| --- | ---: | ---: | ---: |
| Local, async 8 RPCs | 22.916 | 36.024 | 1.572 |
| Local, async 16 RPCs | 23.552 | 47.821 | 2.030 |
| Local, async 32 RPCs | 21.835 | 47.266 | 2.165 |
| Delayed, async 8 RPCs | 1.345 | 2.681 | 1.993 |
| Delayed, async 16 RPCs, clean repeat | 1.334 | 4.504 | 3.375 |
| Delayed, async 32 RPCs | 1.355 | 4.582 | 3.381 |
| Local, both 16 RPCs, sync 16 / async 4 workers | 46.349 | 48.633 | 1.049 |

Except the final control, sync uses four workers and eight RPC slots. Therefore
the first six comparisons include the synchronous callback-worker bottleneck;
they do not isolate an intrinsic asynchronous transport speedup. The matched
control shows similar throughput with fewer callback workers, not a universal
twofold asynchronous advantage.

Keep four Linux callback workers, eight RPC slots, 32 MiB read budget and 64
pending reads as conservative defaults. Worker and RPC counts are independent.
Sixteen RPC slots are a useful opt-in for parallel distinct-file workloads in
this environment; 32 showed little additional benefit. Do not derive these
limits mechanically from OS CPU count or describe eight as a protocol limit.
The configurable RPC ceiling remains 64. Linux auto mode uses asynchronous
dispatch; macOS auto remains sync and Windows retains native WinFsp dispatch.
Native platform verification remains a separate release requirement.

Regressions are retained: clean delayed same-file 16-reader throughput was
8.835 sync versus 8.607 async ops/s (-2.58%). All per-workload trial values are
included, rather than selecting only improvements. Candidate sampled mount RSS
maxima across primary matrices were 15,084–17,392 KiB. These samples and whole
container cgroup memory/CPU records are observations, not hard RSS bounds or
per-operation CPU attribution.

The original delayed 16-RPC matrix overlapped a 19.58-second contract-test
compilation during its first pair. Its 72 cases are retained separately in the
same evidence file, giving 576 total recorded cases, but it is excluded from
the primary set and replaced by the complete clean repeat. These performance
records do not substitute for strict shutdown, zero-deadline cancellation,
interrupt, saturation, replacement-mount, native or exact-release gates.

Recheck retained data with `scripts/performance/summarize-production-reads.py`.
It verifies binary equality, case dimensions, individual ops/s arithmetic,
trial membership and medians; payload verification belongs to the original
live harness. Never infer payload correctness solely from the summary checker.
