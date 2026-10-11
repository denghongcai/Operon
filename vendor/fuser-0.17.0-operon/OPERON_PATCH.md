# Operon fuser patch

This directory vendors `fuser` 0.17.0 with macOS FUSE-T compatibility and
the source-stage asynchronous-read lifecycle changes described below.

Changes from upstream 0.17.0:

- macOS default `INIT_FLAGS` only advertises `FUSE_ASYNC_READ`.
- macOS init replies do not OR in Linux `FUSE_INIT_EXT`.
- macOS libfuse2 mounting uses the current `fuse_mount()` / `fuse_chan_fd()`
  path instead of the legacy `fuse_mount_compat25()` raw-fd path, preserving
  FUSE-T's channel monitor/callback behavior.
- The macOS `fuse_chan` handle is stored as an opaque integer inside fuser's
  mount state so the background session thread still satisfies Rust's `Send`
  bound.
- macOS receives exactly one FUSE-T stream-socket request per session-loop
  iteration by reading the FUSE header first, then the remaining
  `fuse_in_header.len` bytes. Linux keeps the upstream single `read()` path
  because `/dev/fuse` preserves request packet boundaries.
- macOS uses FUSE-T's 32 MiB Darwin user/kernel buffer size for negotiated
  `max_write`; Linux keeps fuser's upstream 16 MiB limit.
- macOS init replies always use the libfuse2/Darwin 24-byte `fuse_init_out`
  payload size. FUSE-T may send an init request with minor 23, but its bundled
  libfuse2 success path still replies with the Darwin libfuse2 init payload,
  not fuser's newer FUSE3-sized init payload.
- macOS build probing does not enable fuser's `macfuse-4-compat` request ABI
  when pkg-config resolves `fuse` to FUSE-T's `-lfuse-t`. FUSE-T 1.2.1
  negotiates a `libfuse2-compatible` session and sends the legacy 8-byte
  `FUSE_RENAME` request payload; the MacFUSE 4 layout would skip the old name
  and decode normal renames as an empty source filename.

Additional source-stage lifecycle changes (not yet published):

- Linux background receivers use nonblocking `/dev/fuse` reads and a shared
  level-triggered stop socket, so shutdown can wake and join every receiver.
  Cloned FUSE descriptors share the stop signal.
- Linux mount identity is captured with a temporary O_PATH descriptor before
  handshake. It is not retained during normal operation, allowing ordinary
  external unmount. Detached-session teardown disarms path-based unmount and
  joins the background session; unmount errors still trigger receiver shutdown
  and join before being returned.
- Owned Linux native shutdown uses lazy detach, matching the existing helper,
  to support user-held open descriptors. An identity check is not an atomic
  unmount: the user accepted disclosing the concurrent privileged overmount
  check-to-syscall limitation. Ordinary detached replacement remains tested.
- Filesystem INTERRUPT dispatch accepts known requests without a separate
  acknowledgement and returns EAGAIN for unknown requests. Operon's scheduler
  owns the interrupted read's reply and cancellation registration.
- Pure-Rust unmount-helper execution errors and unsuccessful exit status are
  propagated instead of silently reported as success.
- macOS FUSE-T stream EOF between complete frames ends the event loop normally.
  Partial-frame EOF is still EIO and nonempty malformed requests remain errors;
  this does not suppress unmount-helper errors or arbitrary invalid frames.

The productionization plan and live evidence are in
`docs/plan/v0.18.24-async-fuse-read-roadmap.md` and
`docs/quality/async-read-lifecycle.md` at the workspace root. macOS compatibility
above remains preserved; Linux evidence does not substitute for native macOS
or Windows acceptance.
