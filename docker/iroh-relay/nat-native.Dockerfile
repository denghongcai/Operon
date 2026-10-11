# Owned acceptance executable only; never shipped or spawned by Operon.
FROM rust:1.91.0-slim-bookworm@sha256:81c97592d519ba7397e8adccb0345fb5a91965c0a44b82af83bd1ef06c36cef5 AS builder
WORKDIR /workspace
RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY proto ./proto
COPY vendor ./vendor
RUN --mount=type=cache,id=operon-nat-acceptance-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=operon-nat-acceptance-target,target=/workspace/target \
    cargo build --release --locked -p operond -p operon-cli \
    && cargo build --release --locked -p operon-transport --features iroh --example nat_acceptance \
    && mkdir /out \
    && cp target/release/operond target/release/operon target/release/examples/nat_acceptance /out/
FROM debian:bookworm-slim
COPY --from=builder /out/ /usr/local/bin/
