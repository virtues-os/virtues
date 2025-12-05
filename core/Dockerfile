# Build stage
FROM rust:1.88-slim AS builder

# Install required dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./

# Copy source code
COPY src ./src
COPY migrations ./migrations
COPY .sqlx ./.sqlx

# Build for release
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies (wget for healthcheck)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    wget \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/virtues /usr/local/bin/virtues
COPY --from=builder /app/migrations ./migrations

# Expose API port
EXPOSE 8000

# Health check (using wget to verify /health endpoint responds correctly)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget -qO- http://localhost:8000/health || exit 1

# Run migrations and start server
CMD ["sh", "-c", "virtues migrate && virtues server"]
