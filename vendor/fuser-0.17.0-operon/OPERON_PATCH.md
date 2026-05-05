# Operon fuser patch

This directory vendors `fuser` 0.17.0 with the smallest macOS handshake patch
needed for the v0.14 FUSE-T live-mount investigation.

Changes from upstream 0.17.0:

- macOS default `INIT_FLAGS` only advertises `FUSE_ASYNC_READ`.
- macOS init replies do not OR in Linux `FUSE_INIT_EXT`.
- macOS libfuse2 mounting uses the current `fuse_mount()` / `fuse_chan_fd()`
  path instead of the legacy `fuse_mount_compat25()` raw-fd path, preserving
  FUSE-T's channel monitor/callback behavior.

The Linux path is intentionally unchanged.
