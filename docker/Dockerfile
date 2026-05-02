FROM rust:1.85-slim AS builder

WORKDIR /workspace

RUN apt-get update \
  && apt-get install -y --no-install-recommends protobuf-compiler \
  && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY proto ./proto

RUN cargo build --release -p operond -p operon-cli

FROM debian:bookworm-slim

RUN useradd --create-home --shell /usr/sbin/nologin operon \
  && mkdir -p /etc/operon \
  && mkdir -p /workspace \
  && chown operon:operon /workspace

COPY --from=builder /workspace/target/release/operond /usr/local/bin/operond
COPY --from=builder /workspace/target/release/operon /usr/local/bin/operon
COPY examples/docker-secrets.yaml /etc/operon/secrets.yaml
COPY examples/docker-node-a-config.yaml /etc/operon/node-a-config.yaml
COPY examples/docker-node-b-config.yaml /etc/operon/node-b-config.yaml
COPY docker/entrypoint.sh /usr/local/bin/operon-docker-entrypoint

RUN chown -R operon:operon /etc/operon /workspace

USER operon

EXPOSE 7789

ENTRYPOINT ["operon-docker-entrypoint"]
