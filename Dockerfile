FROM rust:1.85.0-bookworm AS builder

WORKDIR /build

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY crates ./crates
COPY migrations ./migrations

RUN cargo build --release --package vestrace-cli --bin vestrace \
    && strip target/release/vestrace

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 vestrace \
    && useradd --system --uid 10001 --gid vestrace --no-create-home --home-dir /nonexistent vestrace

COPY --from=builder --chown=vestrace:vestrace /build/target/release/vestrace /usr/local/bin/vestrace

USER vestrace:vestrace
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/vestrace"]
CMD ["server"]
