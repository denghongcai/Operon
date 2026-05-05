# Operon fuser patch

This directory vendors `fuser` 0.17.0 with the smallest macOS handshake patch
needed for the v0.14 FUSE-T live-mount investigation.

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

The Linux path is intentionally unchanged.
