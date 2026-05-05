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

The Linux path is intentionally unchanged.
