FROM rust:1.85.0-bookworm AS builder

WORKDIR /build

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY crates ./crates
COPY migrations ./migrations

# Compiled into the binary, not read at runtime.
# `crates/vestrace-infrastructure/src/openai_q1.rs` embeds the q1 manifest and
# the q1 marker fixture with `include_str!`/`include_bytes!`, so the build fails
# without them even though nothing here executes a test. Both are protected
# authority paths: the image must carry the same bytes the qualification pinned,
# which is why they are copied rather than duplicated into the crate.
COPY schemas ./schemas
COPY tests/fixtures ./tests/fixtures

# The revision this binary was built from, compiled in.
#
# A capability manifest refuses to be written without one, because a
# qualification that cannot say which source it describes certifies nothing in
# particular. The argument is referenced by the build command itself rather than
# set as an `ENV` above it: a layer rebuilds when something it references
# changes, and an `ENV` the `RUN` never mentions leaves the compile cached with
# whatever revision was baked in last time.
ARG VESTRACE_SOURCE_REVISION=unknown-source-revision

RUN VESTRACE_SOURCE_REVISION="${VESTRACE_SOURCE_REVISION}" \
    cargo build --release --package vestrace-cli --bin vestrace \
    && strip target/release/vestrace

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 vestrace \
    && useradd --system --uid 10001 --gid vestrace --no-create-home --home-dir /nonexistent vestrace

COPY --from=builder --chown=vestrace:vestrace /build/target/release/vestrace /usr/local/bin/vestrace

USER vestrace:vestrace
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/vestrace"]
CMD ["server"]
