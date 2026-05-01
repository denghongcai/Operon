FROM rust:1.85-slim AS builder

WORKDIR /workspace

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release -p operond -p operon-cli

FROM debian:bookworm-slim

RUN useradd --create-home --shell /usr/sbin/nologin operon \
  && mkdir -p /etc/operon \
  && mkdir -p /workspace \
  && chown operon:operon /workspace

COPY --from=builder /workspace/target/release/operond /usr/local/bin/operond
COPY --from=builder /workspace/target/release/operon /usr/local/bin/operon
COPY examples/docker-policy.yaml /etc/operon/policy.yaml

USER operon

EXPOSE 7788

ENTRYPOINT ["operond"]
