# Multi-target runtime Dockerfile (no in-container build)
# - base: Common runtime dependencies and assets
# - cli:  Runs the prebuilt bingle_cli binary with docker_start.sh
# - webserver: package the prebuilt bingle_webserver and start script
# - gateway: bingle_cli logclient + scripts/mailer.sh, for forwarding messages to email

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
COPY nodely_deployed_mainnet_node.json /app/nodely_deployed_mainnet_node.json

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

# Copy the prebuilt binary and startup script, executable. --chmod sets the mode as it copies; a
# separate RUN chmod would store the whole binary again in a new layer.
COPY --chmod=755 ${BIN_PATH} /app/bingle_cli
COPY --chmod=755 scripts/docker_start.sh /app/docker_start.sh

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

# Copy the prebuilt binary and startup script, executable (see the cli stage)
COPY --chmod=755 ${WEB_BIN_PATH} /app/bingle_webserver
COPY --chmod=755 scripts/docker_webserver_start.sh /app/docker_webserver_start.sh

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
# Gateway stage: forwards Bingle messages to email (epic #63, issue #473). One image, two roles:
# `logclient` writes each message into a Maildir on the shared volume, and `mailer` sends it through
# the SES API with the task role, which needs the AWS command-line tool.
# ------------------------
FROM base AS gateway

RUN dnf install -y awscli-2 && dnf clean all

ARG BIN_PATH=target/aarch64-unknown-linux-musl/debug/bingle_cli

COPY --chmod=755 ${BIN_PATH} /app/bingle_cli
COPY --chmod=755 scripts/mailer.sh /app/mailer.sh
COPY --chmod=755 scripts/docker_gateway_start.sh /app/docker_gateway_start.sh

ENV NODE_FILE="/app/nodely_deployed_mainnet_node.json" \
    STUN_FILE="/app/stunservers.txt"

# The role (logclient or mailer) is the container's command.
ENTRYPOINT ["/app/docker_gateway_start.sh"]

# ------------------------
# Default final image is the CLI runtime, to preserve previous behavior
# ------------------------
FROM cli
