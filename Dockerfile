FROM rust:latest AS builder

WORKDIR /app

# Copy only files needed for dependency resolution first
COPY Cargo.toml Cargo.lock ./

RUN rustup target add x86_64-unknown-linux-musl && \
    apt update && \
    apt install -y musl-tools musl-dev && \
    update-ca-certificates

# Copy the rest of the files and build the real application
COPY ./src ./src
RUN cargo build --target x86_64-unknown-linux-musl --release

# Check that the executable exists
RUN test -f /app/target/x86_64-unknown-linux-musl/release/jellyfin_pr_migration

FROM alpine:latest

# Create app and data directories with proper permissions
WORKDIR /app
RUN mkdir -p /data && chown -R 144:153 /data
RUN mkdir -p /app && chown -R 144:153 /app

# Copy the built executable from the builder stage
COPY --from=builder --chown=144:153 /app/target/x86_64-unknown-linux-musl/release/jellyfin_pr_migration /app/jellyfin_pr_migration

# Check that the executable exists
RUN test -f /app/jellyfin_pr_migration

# Set the data directory as a volume
VOLUME /data

# Change to the non-root user
USER 144:153

# Add metadata labels
LABEL org.opencontainers.image.title="Jellyfin PlaybackReporting Migration Tool"
LABEL org.opencontainers.image.description="Tool for migrating Jellyfin PlaybackReporting data between instances"
LABEL org.opencontainers.image.source="https://github.com/wolffshots/jellyfin_pr_migration"

# Check that the executable exists
RUN test -f /app/jellyfin_pr_migration

ENV RUST_LOG=info
ENV RUST_LOG_STYLE=always

# Run the migration tool
# Users should mount their config.toml, input.tsv, and optionally playback_reporting.db to /data
ENTRYPOINT ["/app/jellyfin_pr_migration", "-c", "/data/config.toml"]
