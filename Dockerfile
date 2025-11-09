# Multi-stage build for sol_beast backend
FROM rust:latest as builder

WORKDIR /usr/src/sol_beast

# Copy manifests
COPY Cargo.toml ./
COPY Cargo.lock* ./

# Build dependencies in a separate layer for caching
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /usr/src/sol_beast/target/release/sol_beast /app/sol_beast

# Copy config and keypair (mount at runtime)
# COPY config.toml ./
# COPY keypair.json ./

EXPOSE 8080

ENV RUST_LOG=info

CMD ["./sol_beast"]
