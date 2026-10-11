# Optional embedded connectivity measurements

These are source acceptance measurements, not a public-release performance
guarantee. All cases use the actual public TypeScript SDK against the same
owned daemon, bearer policy and file contents. The gRPC control uses nice-grpc;
the embedded cases use the in-process native binding. Thus the comparison
includes SDK adapter overhead as well as transport overhead.

## Local acceptance, 2026-10-11

Linux x86_64, Node v24.11.0, Rust 1.91 debug builds, one sequential caller,
five warmups and three repeats of 200 operations per workload. Range reads
verify all 4096 returned bytes. The isolated relay is self-hosted on localhost;
both embedded endpoints disable IP transports in the relay case.

Median operations per second:

| Run | Backend | Capability requests | 4 KiB range reads |
| --- | --- | ---: | ---: |
| Direct comparison | gRPC control | 1597.5 | 1385.7 |
| Direct comparison | iroh direct | 410.6 | 394.9 |
| Relay comparison | gRPC control | 1570.8 | 1415.4 |
| Relay comparison | iroh relay | 328.0 | 299.1 |

Embedded requests are slower than the gRPC control in these small sequential
workloads. This supports keeping embedded connectivity opt-in. It does not
measure WAN latency, NAT performance, concurrent workloads, FUSE throughput,
release-build throughput or the previous published daemon's performance.
Original behavior and regression acceptance remain separate release gates.

Evidence: `/tmp/operon-sdk-direct-ops.log` and
`/tmp/operon-sdk-relay-ops.log`, including each repeat's elapsed time, operation
count and ops/s. Both runs also pass the public SDK filesystem, exec, service,
PTY, authorization, cancellation and lost-peer checks. The relay wrapper passes
its two actual relay/path-upgrade tests and byte-counter checks.

Reproduce after building the SDK, daemon and Node binding:

```sh
export OPERON_SDK_BENCH_OPS=200
export OPERON_SDK_NATIVE_PATH="$PWD/target/debug/liboperon_sdk_native.so"
export OPERON_EMBEDDED_TEST_TIMEOUT_SECS=180
node --expose-gc scripts/verify-sdk-native.mjs
OPERON_RELAY_TEST_IMAGE=operon-iroh-relay-acceptance \
  bash scripts/verify-embedded-relay.sh
```

The relay image must first be built from `docker/iroh-relay`. Use the platform's
matching binding path. Keep payload sizes, builds, caller count and operation
count matched when comparing runs. Final exact-source release acceptance is
still required.
