FROM rust:1.85-slim AS builder

WORKDIR /workspace

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release -p operond -p operon-cli

FROM debian:bookworm-slim

RUN useradd --create-home --shell /usr/sbin/nologin operon

COPY --from=builder /workspace/target/release/operond /usr/local/bin/operond
COPY --from=builder /workspace/target/release/operon /usr/local/bin/operon

USER operon

EXPOSE 7788

ENTRYPOINT ["operond"]
