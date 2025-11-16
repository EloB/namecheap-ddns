# ---- Build stage ------------------------------------------------------------
FROM rust:1.85-slim-bookworm AS builder

# Install minimal tools (pkg-config is usually enough for Rust deps)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Pre-build step to cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src target/release/deps/*

# Real source
COPY src ./src
RUN cargo build --release

# Prepare writable /data directory (owned by nonroot for distroless)
RUN mkdir -p /app/data

# ---- Runtime stage (distroless) --------------------------------------------
FROM gcr.io/distroless/cc-debian12

WORKDIR /

# Copy binary
COPY --from=builder /app/target/release/namecheap-ddns /namecheap-ddns

# Copy prepared /data folder with correct ownership
COPY --from=builder --chown=nonroot:nonroot /app/data /data

USER nonroot:nonroot

VOLUME ["/data"]
ENTRYPOINT ["/namecheap-ddns"]