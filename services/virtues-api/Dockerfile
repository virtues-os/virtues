# virtues-api — AI budget/usage proxy (the usage side of the wall).
#
# Workspace member with a path dep on crates/virtues-helpers, so it builds from
# the REPO ROOT context (not services/virtues-api):
#   docker build -f services/virtues-api/Dockerfile -t virtues-api .
#
# cargo resolves the whole workspace but compiles only virtues-api + its dep
# graph (core/ort are never built). target/ and node_modules/ are excluded by
# the repo-root .dockerignore.

# ── Stage 1: build ──────────────────────────────────────────────────────────
FROM rust:1.94-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /repo
COPY . .
# Pin the target dir explicitly. A developer checkout carries an untracked
# `.cargo/config.toml` pointing `build.target-dir` at a shared cache outside the
# repo (~/.cargo/shared-target — see CLAUDE.md), and if that file reaches the
# build context, cargo writes the binary to a host path that does not exist in
# this image and the COPY below fails. CI never saw it (fresh checkouts have no
# such file), so only local `make deploy-virtues-api` broke — the one path that
# builds the image prod actually pulls. The env var wins over any config.
ENV CARGO_TARGET_DIR=/repo/target
RUN cargo build --release -p virtues-api

# ── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    wget \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -u 1000 -U virtues-api

COPY --from=builder /repo/target/release/virtues-api /usr/local/bin/virtues-api

USER virtues-api

# VIRTUES_API_PORT (default 9002)
EXPOSE 9002

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://localhost:9002/health || exit 1

CMD ["virtues-api"]
