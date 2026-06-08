# ==============================================================================
# Stage 1: Build SvelteKit static frontend
# ==============================================================================
FROM node:20-slim AS web-builder

ARG GIT_COMMIT=dev
ENV GIT_COMMIT=${GIT_COMMIT}

RUN npm install -g pnpm

WORKDIR /app

# Copy package files first for layer caching
COPY apps/web/package.json apps/web/pnpm-lock.yaml apps/web/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile

# Copy source and build
COPY apps/web/ ./
RUN pnpm build

# ==============================================================================
# Stage 2: Plan Rust dependencies (cargo-chef, workspace-aware)
# ==============================================================================
FROM rust:1.94-slim AS planner

RUN cargo install cargo-chef

WORKDIR /repo

# Full workspace skeleton — root manifest + all members. `edition.workspace = true`
# in member Cargo.tomls requires the workspace root to be present.
COPY Cargo.toml Cargo.lock ./
COPY virtues-core /repo/virtues-core
COPY crates /repo/crates
COPY actions /repo/actions
COPY services /repo/services
COPY .sqlx /repo/.sqlx

RUN cargo chef prepare --recipe-path recipe.json

# ==============================================================================
# Stage 3: Cook dependencies + build (Ubuntu 24.04 for glibc 2.39 / ort-sys)
# ==============================================================================
FROM ubuntu:24.04 AS builder

ARG GIT_COMMIT=unknown
ENV GIT_COMMIT=${GIT_COMMIT}
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.94.0
ENV PATH="/root/.cargo/bin:${PATH}"

RUN cargo install cargo-chef

WORKDIR /repo

# Cook dependencies for the `virtues` package only (path-dep transitive
# closure: virtues-helpers, virtues-wg, virtues-registry). chef regenerates
# the workspace skeleton from recipe.json — we do NOT pre-copy any source.
# This layer is cached and only invalidates when recipe.json changes (i.e.
# when any Cargo.toml or Cargo.lock changes).
COPY --from=planner /repo/recipe.json recipe.json
RUN cargo chef cook --release -p virtues --recipe-path recipe.json

# Now bring the real workspace source on top — overwrites the chef stubs.
# This layer reruns on any source change but reuses the cooked deps above.
COPY Cargo.toml Cargo.lock ./
COPY virtues-core /repo/virtues-core
COPY crates /repo/crates
COPY actions /repo/actions
COPY services /repo/services
COPY .sqlx /repo/.sqlx

RUN cargo build --release -p virtues

# ==============================================================================
# Stage 3b: Bake the embedder + reranker ONNX models into /opt/virtues/models
# Runs in a thin layer so it's cached unless deploy/fetch-models.sh changes.
# Files land at /opt/virtues/models/<repo_basename>/onnx/model_int8.onnx
# which matches the lookup order in virtues-core/src/search/model_cache.rs.
# ==============================================================================
COPY deploy/fetch-models.sh /usr/local/bin/fetch-models.sh
RUN chmod +x /usr/local/bin/fetch-models.sh && \
    /usr/local/bin/fetch-models.sh /opt/virtues/models

# ==============================================================================
# Stage 4: Runtime (Ubuntu 24.04 for glibc 2.39 compat with ort-sys)
# ==============================================================================
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3t64 \
    libpq5 \
    wget \
    openssh-server \
    python3 \
    python3-pip \
    python3-venv \
    && rm -rf /var/lib/apt/lists/*

# sshd: create privilege separation directory and hardened config
RUN mkdir -p /run/sshd && \
    printf '%s\n' \
      'PermitRootLogin no' \
      'PasswordAuthentication no' \
      'PubkeyAuthentication yes' \
      'AllowUsers virtues' \
      'X11Forwarding no' \
      'PrintMotd no' \
      'AcceptEnv LANG LC_*' \
    > /etc/ssh/sshd_config.d/virtues.conf

# Python packages for code interpreter (gVisor provides isolation)
RUN pip3 install --no-cache-dir --break-system-packages \
    numpy \
    pandas \
    scipy \
    matplotlib \
    requests \
    httpx \
    python-dateutil \
    pytz

# Create non-root user with home directory and login shell
# Remove Ubuntu's default 'ubuntu' user (UID 1000) first, then create ours
RUN userdel -r ubuntu 2>/dev/null || true && \
    useradd -m -u 1000 -U -s /bin/bash virtues

WORKDIR /app

# Copy Rust binary (built into the workspace target/), migrations, and seeds
COPY --from=builder /repo/target/release/virtues /usr/local/bin/virtues
COPY --from=builder /repo/virtues-core/migrations ./migrations
COPY --from=builder /repo/virtues-core/seeds ./seeds

# Copy pre-downloaded embedder + reranker ONNX models. The runtime reads
# VIRTUES_MODELS_DIR (defaults to /opt/virtues/models) to find them — no
# HuggingFace download on first boot.
COPY --from=builder /opt/virtues/models /opt/virtues/models
ENV VIRTUES_MODELS_DIR=/opt/virtues/models

# Copy SvelteKit static build
COPY --from=web-builder /app/build/ /app/static/

# Copy entrypoint script
COPY virtues-core/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Set ownership (container starts as root, entrypoint drops to virtues user)
RUN chown -R virtues:virtues /app /home/virtues

EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
    CMD wget -qO- http://localhost:8000/health || exit 1

CMD ["/usr/local/bin/entrypoint.sh"]
