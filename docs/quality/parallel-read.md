# Parallel read tuning and acceptance

v0.16.13 release preparation: mount worker/read budgets and
single-task daemon range reads. Protocol and existing consistency remain unchanged.

```yaml
client:
  mount:
    worker_threads: 8          # Linux FUSE only; omit for the platform default
    max_inflight_reads: 8      # 1–64, default 8
    max_inflight_read_mib: 32  # 8–512 MiB, default 32
  nodes:
    remote:
      endpoint: grpc://remote:7789
      transport:
        rpc_timeout_secs: 300
```

Overrides (use `operon mount --help` for the authoritative syntax):

```sh
operon --rpc-timeout-secs 300 mount remote:/ --to /mnt/remote \
  --mount-workers 8 --read-concurrency 8 --read-budget-mib 32
```

Linux defaults to four synchronous FUSE workers; macOS keeps its supported
single-worker adapter, Windows keeps WinFsp dispatch. Explicit worker count on
non-Linux adapters fails rather than being ignored. Read budgets apply to all
adapters. Queue wait plus RPC use the same ordinary RPC deadline, including CLI
overrides; zero disables the deadline. This bounds the client's read-budget
queue, not time already spent waiting in the kernel/FUSE dispatch queue.
Read failures never replay mutations.
Budgets cover active RPC requested bytes, not replies already handed to the
adapter, process RSS, protobuf temporaries or kernel page cache.
Requested sizes are rounded up to KiB for conservative accounting that also
supports the maximum setting on 32-bit ARMv7.

The daemon opens a fresh descriptor per range request and performs open, seek,
short-read/Interrupted retry, EOF truncation and close in one blocking task.
Authorization, workspace resolution and audit remain per-request; no persistent
file descriptor/content cache, speculative read or connection pool is introduced.
Ordinary local file I/O remains subject to OS blocking-I/O behavior; this is not
a new cancellable disk-I/O guarantee.

## Reproduction

Build baseline before editing and candidate after editing with the same toolchain:

```sh
cargo +1.88.0 build --release --locked -p operon-cli -p operond
python3 scripts/performance/parallel-read.py \
  --baseline /path/to/unchanged/binaries --candidate target/release \
  --readers 1 2 4 8 16 --mib 64 --repeats 3
```

Requires Docker, `/dev/fuse`, and an existing image with Python and fuse3
(default `operon-review-tools:latest`). `--delay-ms 20` injects 20 ms client
egress delay in an isolated container; it installs iproute2 in that disposable
container if needed, not on the host. `--mount-args --mount-workers 8` applies
only to the candidate. A fresh client mount is used for every measured case;
the daemon's OS page cache is deliberately warm, not globally flushed.
Same-file parallel readers share a client cache. Random reads can also hit
page cache; per-case audited read RPC counts and network bytes make that visible.
One sequential operation is one verified complete file; one random operation
is one verified 4KiB pread (not necessarily one RPC). Batch throughput is not
per-read p95 latency; reader elapsed times are recorded separately.

The harness alternates baseline/candidate order, records binary hashes, source
state, RPC counts and cgroup resources, verifies offset-sensitive patterned
payloads and gzip-compresses owned payloads after each case. It preserves all
evidence and cleans only containers/networks created by the run. Failed trials
are not counted as throughput success. Record medians, all trials and regressions
before claiming improvements. Native platform/public release gates remain
separate from local source tests.

## Measured acceptance — 2026-10-09

Baseline source `267897ddf7424240a698fde310f47f7020741e1a` was built before
edits, candidate is the uncommitted implementation (not a published version).
Both use Rust 1.88 release mode. Final candidate uses **8 FUSE workers, 8 active
read RPCs, 32 MiB requested-byte budget**; baseline retains four workers.
Numbers below are medians of three alternating trials, ratios of those medians.
Absolute throughput is host-dependent reference evidence.

| Workload | Baseline ops/s | Candidate ops/s | Ratio |
| --- | ---: | ---: | ---: |
| Local distinct 64MiB files, one reader | 2.74 | 2.83 | 1.034x |
| Local distinct files, two readers | 4.41 | 4.67 | 1.060x |
| Local distinct files, four readers | 4.73 | 7.48 | 1.583x |
| Local distinct files, eight readers | 4.83 | 7.83 | 1.621x |
| Local distinct files, sixteen readers | 4.82 | 7.48 | 1.552x |
| Local random 4KiB reads, one reader | 796.50 | 840.53 | 1.055x |
| Local random reads, two readers | 1705.60 | 1621.04 | **0.950x** |
| Local random reads, four readers | 2877.82 | 3180.81 | 1.105x |
| Local random reads, eight readers | 2972.01 | 5281.92 | 1.777x |
| Local random reads, sixteen readers | 3339.92 | 5780.21 | 1.731x |
| 20ms egress delay, distinct 16MiB files, one reader | 0.638 | 0.640 | 1.004x |
| Delayed distinct files, four readers | 1.339 | 2.449 | 1.829x |
| Delayed distinct files, eight readers | 1.365 | 2.594 | 1.900x |
| Delayed random reads, one reader | 43.37 | 43.26 | **0.998x** |
| Delayed random reads, four readers | 166.14 | 166.12 | **1.000x** |
| Delayed random reads, eight readers | 178.35 | 333.67 | 1.871x |

Same-file cache-sharing results and all individual trials, including regressions,
are retained in [machine-readable evidence](evidence/parallel-read-2026-10-09.json).
This is not a universal speedup claim: local two-reader random throughput fell
about 5%, delayed low-concurrency random reads were essentially unchanged.
Linux's default remains four workers; eight is an explicit tuning option.
An earlier matched four-worker candidate run improved the target distinct-file
four-reader workload from 5.21 to 6.22 ops/s (+19%), while same-file eight-reader
cache-sharing fell 4.7%. That preliminary variant predates the final KiB budget
accounting; its separate hashes/raw evidence are in
`/tmp/operon-parallel-matched-local`. Do not mix absolute values across runs.

A cold client 64MiB single file produced 512 audited range RPCs (128KiB average
payload per RPC). The final four-file local case still produced 2048 RPCs:
gains do not rely on skipping reads/audit, cache weakening or larger ranges.
At sixteen distinct files request counts can increase due to kernel request
splitting/readahead; counters remain separate from completed-file ops/s.
No kernel request/queue parameter was changed, and async FUSE dispatch/caches
remain evidence-dependent future options rather than unfinished implementation.
One final local candidate trial recorded daemon cgroup peak 29,552,640 bytes
versus baseline 27,475,968 bytes; its four-reader mount peak RSS was 14,112 KiB.
These are observed resource values, not total-RSS guarantees from the read budget.

Raw final evidence: `/tmp/operon-parallel-final-local` and
`/tmp/operon-parallel-final-delay20`; each includes binary hashes, source dirty
state, complete counters and verified payloads (recoverably gzip-compressed).
Final candidate hashes are pinned in the machine-readable evidence.

Full local Rust workspace, queue cancellation/config/CLI/range edge tests,
strict clippy/format, docs/help/skills sync, 53 core / 18 runtime / 13 SDK / four
Linux system scripts (real FUSE read/write, no skips), SDK 23 tests and
typecheck/build, production dependency audit and macOS/Windows source checks
passed. The ignored workspace resolver benchmark was run explicitly in both
legacy and cached modes. Network/ACL tests first failed under sandbox restrictions
and passed with host permissions; these were not suppressed code failures.
Native macOS/Windows/Alpine execution and public release verification were **not
run for this change**, and remain required before publishing a new release.
