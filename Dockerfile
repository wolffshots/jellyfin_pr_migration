FROM rust:latest AS builder

WORKDIR /app

RUN rustup target add x86_64-unknown-linux-musl && \
    apt update && \
    apt install -y musl-tools musl-dev && \
    update-ca-certificates

# Copy files needed for dependency resolution and source files
COPY Cargo.toml Cargo.lock ./
COPY ./src ./src

# Build the actual app with a target for x86_64-unknown-linux-musl to be statically linked
RUN cargo build --target x86_64-unknown-linux-musl --release

# Check that the correct executable exists
RUN test -f /app/target/x86_64-unknown-linux-musl/release/jellyfin_pr_migration

FROM scratch

WORKDIR /app

# Copy the built executable from the builder stage with permissions for jellyfin user and group
COPY --from=builder --chown=144:153 /app/target/x86_64-unknown-linux-musl/release/jellyfin_pr_migration /app/jellyfin_pr_migration

VOLUME /data

# Change to the non-root user (jellyfin equivalent)
USER 144:153

LABEL org.opencontainers.image.title="Jellyfin PlaybackReporting Migration Tool"
LABEL org.opencontainers.image.description="Tool for migrating Jellyfin PlaybackReporting data between instances"
LABEL org.opencontainers.image.source="https://github.com/wolffshots/jellyfin_pr_migration"

# Not sure if necessary but Rust didn't want to println! to the virtual tty before
ENV RUST_LOG=info
ENV RUST_LOG_STYLE=always

# Run the migration tool
# Users should mount their config.toml, input.tsv, and optionally playback_reporting.db to /data
ENTRYPOINT ["/app/jellyfin_pr_migration", "-c", "/data/config.toml"]
