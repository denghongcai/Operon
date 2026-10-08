# Alpine and OpenRC service management

Status: source implementation under acceptance; public musl assets and OpenRC
support are not released until Phase 141 passes. The current published Linux
glibc archives do not become Alpine-compatible through installing OpenRC.

The supported acceptance targets are native x86_64 and arm64 on pinned Alpine
3.22 and 3.23. Use the corresponding fully static musl archive when released.
Live mount still requires `/dev/fuse` and `fuse3` (including `fusermount3`) for
non-root mounts. OpenRC is optional: `operond start` remains foreground-only.

## Scope and account

Linux auto-selection checks the active init environment and available tools,
not libc. `--backend systemd` retains the existing systemd **user** service;
`--backend openrc` selects an OpenRC **system** service. Ambiguous environments
require explicit selection. macOS launchd and Windows SCM remain unchanged.

OpenRC installation/control needs root. Operon never runs sudo or creates a
service account automatically. Create an existing non-root account explicitly,
then make its private config/token/secrets readable by that account, normally
mode `0600`, and its existing workspace/store directory writable. The private
files must be owned by the account. Avoid root-owned configuration copied from
an administrator's onboarding directory. `token_env` is rejected for managed
OpenRC services: use an inline token or a private token file, not root's
inherited environment. Service files contain paths/account names, not tokens.

Run these commands with the necessary system-service privileges:

```sh
operond service install --backend openrc --config /home/operon/config.yaml --service-user operon
operond service start --backend openrc
operond service status --backend openrc --json
operond service stop --backend openrc
operond service uninstall --backend openrc
```

Install enables only `operond` in the `default` boot runlevel; it does not start
the daemon. OpenRC `supervise-daemon` runs the foreground daemon as the selected
account, using its home directory and a private umask. It respawns crashed
daemons (default delay 2 seconds, maximum 5 restarts per 60-second period).
Configure these at installation using `--respawn-delay-secs`, `--respawn-max`
(0 means unlimited) and `--respawn-period-secs` (0 disables the period).

Start verifies the actual supervised non-root daemon and authenticated gRPC
health, not merely rc-service's exit status or an unrelated running endpoint.
`status --json` reports installed/running/healthy and the native OpenRC status
code; stopped/unhealthy diagnostic state is successful structured output.
Without JSON, stopped/unhealthy status is a nonzero command. Operational errors
are nonzero with stderr diagnostics. JSON service results currently require
the OpenRC backend; legacy backends retain their existing output contracts.

## Timeouts and cleanup

`--timeout-secs` controls OpenRC command/readiness waits (default 60).
`--stop-timeout-secs` controls the stop-command wait (default 30); **at install**
it also records OpenRC's native TERM/KILL grace schedule. Changing a stop command
deadline does not rewrite the already-running supervisor's native schedule:
reinstall explicitly to change that schedule. Zero disables the corresponding
wait; a zero installation stop timeout uses an indefinite native TERM wait.
Deadlines accept up to seven days for slow environments.

The foreground daemon handles SIGINT/SIGTERM, rejects new exec registrations on
shutdown and cancels/waits for active exec and PTY tasks. Its complete graceful
shutdown deadline is configurable with `operond start --shutdown-timeout-secs`
(default 30; 0 waits indefinitely), or the same flag on OpenRC service install.
Configure an adequate supervisor grace period
when using a larger daemon deadline; forcible termination cannot promise graceful
child cleanup. Existing Windows SCM shutdown invokes the same runtime cleanup.

Reinstall validates the existing owned entry and stops it before replacement;
start is a separate operation. Conflicting/modified/symlinked service files are
not overwritten or controlled. Uninstall removes only Operon's owned init
script, registration and default-runlevel entry. It preserves configuration,
tokens, workspace, persisted store and logs. Other runlevels/services are not
modified automatically.

Logs are private files in `/var/log/operon/stdout.log` and `stderr.log`.
Inspect `rc-service operond status`, the private daemon logs and
`operon --config <path> doctor` on failures. A stale registration or changed
account identity requires explicit review/reinstallation, not automatic takeover.
The fallback is foreground `operond start --config <path>`.

## Acceptance

`scripts/run-alpine-openrc-acceptance.sh <binary-directory> <pinned-image>` boots
Alpine's real BusyBox PID 1 and OpenRC sysinit/boot/default chain inside an owned
disposable container. It verifies account/permission/conflict errors, paths with
spaces/quotes/metacharacters, real supervision, readiness, crash respawn,
exec-child shutdown, then actually restarts the container to verify boot enablement,
persisted state and upgrade/uninstall. No fake supervisor or softlevel marker
creation substitutes for boot. Required native CI runs both Alpine releases on
x86_64 and arm64; missing runtime privileges are failures, not skips.

Harness waits are configurable via `OPERON_OPENRC_ACCEPT_TIMEOUT_SECS` and
`OPERON_OPENRC_STOP_TIMEOUT_SECS`. Its scoped Docker capabilities/tmpfs never
expose host service directories, PID namespace or writable source mounts.
