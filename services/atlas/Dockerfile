# atlas — identity + billing (the Stripe customer side of the wall).
#
# atlas is a workspace member with a path dependency on crates/virtues-helpers,
# so it must build from the REPO ROOT context (not services/atlas):
#
#   docker build -f services/atlas/Dockerfile -t atlas .
#
# cargo resolves the whole workspace but compiles only atlas + its dep graph
# (core/ort are never built — atlas doesn't depend on them). target/ and
# node_modules/ are excluded by the repo-root .dockerignore.

# ── Stage 1: build ──────────────────────────────────────────────────────────
FROM rust:1.94-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /repo
COPY . .
RUN cargo build --release -p atlas

# ── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    wget \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -u 1000 -U atlas

COPY --from=builder /repo/target/release/atlas /usr/local/bin/atlas

USER atlas

# ATLAS_PORT (default 9100)
EXPOSE 9100

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://localhost:9100/health || exit 1

CMD ["atlas"]
