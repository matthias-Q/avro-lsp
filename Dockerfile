# Multi-stage build for avro-lsp linter
# Stage 1: Build the binary
FROM rust:1.93-slim AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy manifest files first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source files to cache dependencies
RUN mkdir -p src benches && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > benches/parser_bench.rs && \
    echo "fn main() {}" > benches/validator_bench.rs && \
    echo "fn main() {}" > benches/handlers_bench.rs && \
    echo "fn main() {}" > benches/workspace_bench.rs && \
    echo "fn main() {}" > benches/integration_bench.rs && \
    cargo build --release && \
    rm -rf src benches

# Copy actual source code
COPY src ./src
COPY benches ./benches

# Build the real binary
RUN cargo build --release --bin avro-lsp

# Stage 2: Create minimal runtime image
FROM debian:trixie-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /build/target/release/avro-lsp /usr/local/bin/avro-lsp

# Set up working directory for linting
WORKDIR /workspace

# Default command: lint current directory with workspace mode
CMD ["avro-lsp", "lint", "--workspace", "."]
