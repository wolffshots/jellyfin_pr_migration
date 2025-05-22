# Build stage
FROM rust:1.73-slim-bullseye as builder

WORKDIR /app

# Copy only files needed for dependency resolution first
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy the rest of the files and build the real application
COPY . .
RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user to run the application
RUN groupadd -r jellyfin && useradd -r -g jellyfin jellyfin

# Create app and data directories with proper permissions
WORKDIR /app
RUN mkdir -p /data && chown -R jellyfin:jellyfin /data

# Copy only the built executable from the builder stage
COPY --from=builder /app/target/release/jellyfin_pr_migration /app/jellyfin_pr_migration

# Set the data directory as a volume
VOLUME /data

# Switch to non-root user
USER jellyfin

# Documentation
LABEL org.opencontainers.image.title="Jellyfin PlaybackReporting Migration Tool"
LABEL org.opencontainers.image.description="A tool to migrate Jellyfin PlaybackReporting data between instances"
LABEL org.opencontainers.image.source="https://github.com/wolffshots/jellyfin_pr_migration"

# Run the migration tool
# Users should mount their config.toml, input.tsv, and optionally the playback_reporting.db to /data
ENTRYPOINT ["/app/jellyfin_pr_migration", "-c", "/data/config.toml"]
