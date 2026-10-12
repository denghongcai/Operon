# Platform Support

Operon's daemon and CLI support Linux, macOS, and Windows. The core runtime
provides filesystem RPCs, exec, services, policy, audit, trace, and execution
graphs on all three platforms. Live mounts use a platform-specific adapter.

## Supported platforms

| Platform | CPU architectures | Release archive | Live mount dependency |
| --- | --- | --- | --- |
| Linux GNU/glibc | `x86_64`, `arm64`, `armv7` | `.tar.gz` | FUSE |
| Linux musl / Alpine | `x86_64`, `arm64` | `.tar.gz` (static binaries) | FUSE |
| macOS | Intel `x86_64`, Apple Silicon `aarch64` | `.tar.gz` | FUSE-T |
| Windows | `x86_64` | `.zip` | WinFsp |

The prebuilt archives include daemon/CLI binaries and platform live mount
adapters. FUSE, FUSE-T, or WinFsp must also be installed on the host; they are
not required for ordinary filesystem RPCs or exec commands. GNU archives do
not run on Alpine—use the musl archive instead.

See [installation](install.md) for archive selection and mount prerequisites.
The [TypeScript SDK](install.md#typescript-sdk) is distributed through npm;
its optional native iroh binding supports the platforms above except ARMv7.

## Check platform readiness

Start with a general diagnostic:

```sh
operon doctor
```

Before mounting, check the local mount runtime. This command does not require
a config file or a reachable daemon:

```sh
operon doctor --mount-runtime
```

Use `operon --json doctor` or `operon --json doctor --mount-runtime` for
machine-readable output. Doctor provides runtime status and install hints,
along with config, endpoint, authentication, protocol, capability, and service
diagnostics. `operon mount` performs the same local preflight before connecting.

## Live mount reads

### Platform defaults

`client.mount.mode: auto` selects the native default:

| Platform | Read dispatch | Platform-specific behavior |
| --- | --- | --- |
| Linux | Asynchronous FUSE reads | Callback workers and in-flight RPCs are limited independently. |
| macOS | Synchronous FUSE reads | Explicit `async` is also supported. |
| Windows | Native WinFsp dispatch | Unix `async` mode and Linux worker settings are rejected. |

### Configuration

Set read limits under `client.mount` in `config.yaml`:

| Setting | Default | Allowed values | Controls |
| --- | --- | --- | --- |
| `max_inflight_reads` | `8` | `1–64` | Active range-read RPCs on all platforms. |
| `max_inflight_read_mib` | `32` | `8–512` MiB | Requested bytes in active read RPCs on all platforms. |
| `mode` | `auto` | `auto`, `sync`, `async` | Unix read dispatch; see platform defaults above. |
| `worker_threads` | `4` on Linux | `1–64` | Linux FUSE callback workers; omit on macOS and Windows. |
| `max_pending_reads` | `64` | `1–4096` | Queued and active owned asynchronous FUSE tasks. |
| `unmount_poll_ms` | `250` | `10–60000` ms | Linux external-unmount detection interval. |

For example, apply read limits and a five-minute RPC deadline to one mount:

```sh
operon --rpc-timeout-secs 300 mount remote:/ --to /mnt/remote \
  --read-concurrency 8 --read-budget-mib 32
```

Additional CLI overrides are `--read-mode`, `--mount-workers`, `--read-pending`,
and `--unmount-poll-ms`. Their platform restrictions match the configuration
settings. Run `operon mount --help` for the complete command reference.

### Limits and cancellation

The byte budget limits active RPC request sizes, not process memory or kernel
page cache. Increasing workers or concurrency is not a universal speedup.
See the [parallel-read measurements](quality/parallel-read.md) and
[production ops/s results](quality/async-read-production.md).

Admission, read-budget queueing, and the RPC share one `rpc_timeout_secs`
deadline. Set a finite value large enough for your link; `0` disables the
deadline and can leave reads waiting indefinitely on a lost connection.
Shutdown cancellation remains active. Linux also detects external unmount
and cancels in-flight reads, including when the RPC deadline is disabled.

Do not concurrently overmount a path while its original owner is shutting
down. Mount identity checks and unmount are not atomic: coordinate replacement
and wait for the original owner to exit. See the
[read configuration and cancellation boundaries](quality/async-read-configuration.md).

## Windows behavior

- Windows non-interactive exec cancellation uses Job Object process-tree
  termination, including descendant processes.
- Windows interactive exec sessions use `portable-pty` and the same
  `OpenExecSession` protocol as Unix-like platforms. Diagnostics report
  `windows-portable-pty-smoke-validated`.
- Windows token and config private-file handling uses ACL-aware validation.
  Generated private files allow the current user, Administrators, and SYSTEM;
  broadly accessible existing private files are rejected.
- Service forwarding can be affected by host firewall rules. Use `operon doctor`
  to check service health and the reported platform caveats.
