# virtues-atlas — identity + billing (the Stripe customer side of the wall).
#
# virtues-atlas is a workspace member with a path dependency on
# crates/virtues-helpers, so it must build from the REPO ROOT context
# (not services/virtues-atlas):
#
#   docker build -f services/virtues-atlas/Dockerfile -t virtues-atlas .
#
# cargo resolves the whole workspace but compiles only virtues-atlas + its dep
# graph (virtues-core / ort are never built — atlas doesn't depend on them).
# target/ and node_modules/ are excluded by the repo-root .dockerignore.

# ── Stage 1: build ──────────────────────────────────────────────────────────
FROM rust:1.94-slim-bookworm AS builder

# build-essential (gcc/make) + cmake + clang are required by aws-lc-sys (pulled in
# by aws-sdk-route53's aws-lc-rs TLS backend) to build the AWS-LC C library and its
# bindings. pkg-config/libssl-dev remain for the rest of the graph.
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    cmake \
    clang \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /repo
COPY . .
RUN cargo build --release -p virtues-atlas

# ── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    wget \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -u 1000 -U virtues

COPY --from=builder /repo/target/release/virtues-atlas /usr/local/bin/virtues-atlas

USER virtues

# VIRTUES_ATLAS_PORT (default 9100)
EXPOSE 9100

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://localhost:9100/health || exit 1

CMD ["virtues-atlas"]
