# PoC-only image: builds the container-mode examples against pinned toolchain.
# Build with the production proto injected as a named context:
#   docker build --build-context proto=<repo>/proto -t operon-iroh-grpc-poc .
FROM rust:1.91.0-slim-bookworm AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
# build.rs resolves ../../proto relative to WORKDIR /build, i.e. /proto.
COPY --from=proto /operon/runtime.proto /proto/operon/runtime.proto
COPY Cargo.toml Cargo.lock build.rs poc.proto /build/
COPY src /build/src
COPY examples /build/examples
RUN cargo build --locked --release --example iroh_server --example iroh_client

FROM debian:bookworm-slim
COPY --from=build /build/target/release/examples/iroh_server /usr/local/bin/
COPY --from=build /build/target/release/examples/iroh_client /usr/local/bin/
