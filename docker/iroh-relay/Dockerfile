# Acceptance infrastructure only: Operon clients never spawn this process.
FROM rust:1.91.0-slim-bookworm@sha256:81c97592d519ba7397e8adccb0345fb5a91965c0a44b82af83bd1ef06c36cef5 AS build
RUN cargo install iroh-relay --version 1.3.0 --locked --features server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/cargo/bin/iroh-relay /usr/local/bin/iroh-relay
COPY acceptance.toml /etc/iroh-relay/acceptance.toml
USER 65534:65534
ENTRYPOINT ["iroh-relay"]
CMD ["--dev", "--config-path", "/etc/iroh-relay/acceptance.toml"]
