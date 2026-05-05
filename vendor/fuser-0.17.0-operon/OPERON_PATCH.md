# Operon fuser patch

This directory vendors `fuser` 0.17.0 with the smallest macOS handshake patch
needed for the v0.14 FUSE-T live-mount investigation.

Changes from upstream 0.17.0:

- macOS default `INIT_FLAGS` only advertises `FUSE_ASYNC_READ`.
- macOS init replies do not OR in Linux `FUSE_INIT_EXT`.

The Linux path is intentionally unchanged.
