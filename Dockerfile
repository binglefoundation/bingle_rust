# Multi-target runtime Dockerfile (no in-container build)
# - base: Common runtime dependencies and assets
# - cli:  Runs the prebuilt bingle_cli binary with docker_start.sh
# - webserver: package the prebuilt bingle_webserver and start script

# Allow selecting target platform when building multi-arch images
ARG TARGETPLATFORM=linux/arm64

# ------------------------
# Base stage with common deps and assets
# ------------------------
FROM --platform=$TARGETPLATFORM public.ecr.aws/amazonlinux/amazonlinux:2023 AS base

# Install minimal runtime dependencies (bash for scripts, CA certs, OpenSSL if dynamically linked)
# Include iproute for IP autodiscovery in docker_start.sh when EXTERNAL_IP is not provided
RUN dnf install -y \
    bash \
    ca-certificates \
    openssl \
    tzdata \
    iproute \
    iptables \
    iptables-nft \
    gawk \
    procps-ng \
  && dnf clean all \
  && update-ca-trust

# App directory
WORKDIR /app

# Copy common runtime assets
COPY stunservers.txt /app/stunservers.txt
COPY nodely_staging_testnet_node.json /app/nodely_staging_testnet_node.json

# Create output directory for test logs (mounted at runtime)
RUN mkdir -p /out /sentinels
VOLUME ["/out", "/sentinels"]

# ------------------------
# CLI stage: package the prebuilt bingle_cli and start script
# ------------------------
FROM base AS cli

# Path to the prebuilt binary within the build context
# Default matches typical cross/zigbuild path; override with --build-arg BIN_PATH=...
ARG BIN_PATH=target/aarch64-unknown-linux-musl/debug/bingle_cli

# Copy the prebuilt binary and startup script
COPY ${BIN_PATH} /app/bingle_cli
COPY scripts/docker_start.sh /app/docker_start.sh

# Ensure the start script and binary are executable
RUN chmod +x /app/docker_start.sh /app/bingle_cli

# Default environment variables (can be overridden at runtime)
ENV PASSPHRASE="" \
    EXTERNAL_IP="" \
    PORT="" \
    STUN_FILE="/app/stunservers.txt" \
    NODE_FILE="/app/nodely_staging_testnet_node.json"

# ENTRYPOINT to the startup script which launches /app/bingle_cli run ...
ENTRYPOINT ["/app/docker_start.sh"]

# ------------------------
# Webserver stage: package the prebuilt bingle_webserver and start script
# ------------------------
FROM base AS webserver

# Path to the prebuilt binary within the build context
ARG WEB_BIN_PATH=target/aarch64-unknown-linux-musl/debug/bingle_webserver

# Copy the prebuilt binary and startup script
COPY ${WEB_BIN_PATH} /app/bingle_webserver
COPY scripts/docker_webserver_start.sh /app/docker_webserver_start.sh

# Ensure the start script and binary are executable
RUN chmod +x /app/docker_webserver_start.sh /app/bingle_webserver

# Default environment variables (can be overridden at runtime)
ENV PORT=12121 \
    ADDRESS="0.0.0.0" \
    PASSPHRASE="" \
    HANDLE="" \
    STUN_FILE="/app/stunservers.txt" \
    NODE_FILE="/app/nodely_staging_testnet_node.json"

# ENTRYPOINT to the startup script which launches /app/bingle_webserver ...
ENTRYPOINT ["/app/docker_webserver_start.sh"]

# ------------------------
# Default final image is the CLI runtime, to preserve previous behavior
# ------------------------
FROM cli
