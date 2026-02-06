# Tollbooth - Open Source AI Budget Proxy
# Multi-stage build with cargo-chef for dependency caching

# ============================================================================
# Stage 1: Plan dependencies (cargo-chef)
# ============================================================================
FROM rust:1.83-slim-bookworm AS planner

RUN cargo install cargo-chef

WORKDIR /app

COPY Cargo.toml Cargo.lock* ./
COPY src ./src

RUN cargo chef prepare --recipe-path recipe.json

# ============================================================================
# Stage 2: Cook dependencies (cached unless Cargo.toml/lock change)
# ============================================================================
FROM rust:1.83-slim-bookworm AS builder

RUN cargo install cargo-chef

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cook dependencies from recipe (cached layer — only reruns when deps change)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Copy actual source and build (only this layer reruns on code changes)
COPY Cargo.toml Cargo.lock* ./
COPY src ./src

RUN cargo build --release

# ============================================================================
# Stage 3: Runtime
# ============================================================================
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -u 1000 -U tollbooth

# Copy binary from builder
COPY --from=builder /app/target/release/tollbooth /usr/local/bin/tollbooth

# Set ownership
RUN chown tollbooth:tollbooth /usr/local/bin/tollbooth

# Switch to non-root user
USER tollbooth

# Expose port
EXPOSE 9000

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget -qO- http://localhost:9000/health || exit 1

# Run tollbooth
CMD ["tollbooth"]
