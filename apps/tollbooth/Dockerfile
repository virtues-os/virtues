# Tollbooth - Open Source AI Budget Proxy
# Multi-stage build for minimal image size

# ============================================================================
# Build Stage
# ============================================================================
FROM rust:1.83-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests first (for better caching)
COPY Cargo.toml Cargo.lock* ./

# Create dummy src to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src target/release/deps/tollbooth*

# Copy actual source
COPY src ./src

# Build release binary
RUN cargo build --release

# ============================================================================
# Runtime Stage
# ============================================================================
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
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
    CMD curl -f http://localhost:9000/health || exit 1

# Run tollbooth
CMD ["tollbooth"]
