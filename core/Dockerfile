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
# Stage 2: Plan Rust dependencies (cargo-chef)
# ==============================================================================
FROM rust:1.88-slim AS planner

RUN cargo install cargo-chef

# Mirror repo layout so path dependencies resolve correctly
WORKDIR /repo/core

COPY core/Cargo.toml core/Cargo.lock ./
COPY core/build.rs ./
COPY core/.cargo ./.cargo
COPY core/src ./src
COPY core/migrations ./migrations
COPY core/.sqlx ./.sqlx
COPY packages/virtues-registry /repo/packages/virtues-registry

RUN cargo chef prepare --recipe-path recipe.json

# ==============================================================================
# Stage 3: Cook dependencies (cached unless Cargo.toml/lock change)
# ==============================================================================
FROM rust:1.88-slim AS builder

ARG GIT_COMMIT=unknown
ENV GIT_COMMIT=${GIT_COMMIT}

RUN cargo install cargo-chef

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    g++ \
    && rm -rf /var/lib/apt/lists/*

# Mirror repo layout for path dependencies
WORKDIR /repo/core

# Copy local path dependency (needed during cook + build)
COPY packages/virtues-registry /repo/packages/virtues-registry
COPY core/.cargo ./.cargo

# Cook dependencies from recipe (cached layer — only reruns when deps change)
COPY --from=planner /repo/core/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code and build (only this layer reruns on code changes)
COPY core/Cargo.toml core/Cargo.lock ./
COPY core/build.rs ./
COPY core/src ./src
COPY core/migrations ./migrations
COPY core/.sqlx ./.sqlx

RUN cargo build --release

# Pre-download the embedding model (~50MB) so runtime doesn't need network access
RUN ./target/release/virtues warm-models

# ==============================================================================
# Stage 4: Runtime
# ==============================================================================
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
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
RUN useradd -m -u 1000 -U -s /bin/bash virtues

WORKDIR /app

# Copy Rust binary and migrations
COPY --from=builder /repo/core/target/release/virtues /usr/local/bin/virtues
COPY --from=builder /repo/core/migrations ./migrations

# Copy pre-downloaded embedding model cache
COPY --from=builder /root/.cache/fastembed /home/virtues/.cache/fastembed

# Copy SvelteKit static build
COPY --from=web-builder /app/build/ /app/static/

# Copy entrypoint script
COPY core/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Set ownership (container starts as root, entrypoint drops to virtues user)
RUN chown -R virtues:virtues /app /home/virtues

EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget -qO- http://localhost:8000/health || exit 1

CMD ["/usr/local/bin/entrypoint.sh"]
