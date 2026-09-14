FROM pgvector/pgvector:pg17-bookworm

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends build-essential libsodium-dev postgresql-server-dev-17 \
    && rm -rf /var/lib/apt/lists/*

COPY vestrace_safety_verify /usr/src/vestrace_safety_verify
RUN make -C /usr/src/vestrace_safety_verify install \
    && rm -rf /usr/src/vestrace_safety_verify