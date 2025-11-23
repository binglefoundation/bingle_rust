# Runtime-only Dockerfile: expects a prebuilt bingle_cli binary in the build context
# No source code COPY and no in-container build.

# Allow selecting target platform when building multi-arch images
ARG TARGETPLATFORM=linux/arm64
FROM --platform=$TARGETPLATFORM public.ecr.aws/amazonlinux/amazonlinux:2023

# Install minimal runtime dependencies (CA certs, OpenSSL if dynamically linked)
RUN dnf install -y \
    ca-certificates \
    openssl \
    tzdata \
  && dnf clean all \
  && update-ca-trust

# App directory
WORKDIR /app

# Path to the prebuilt binary within the build context
# Default matches `cargo build --release --bin bingle_cli` on the host.
ARG BIN_PATH=target/release/bingle_cli

# Copy the prebuilt binary and runtime assets only
COPY ${BIN_PATH} /app/bingle_cli
COPY stunservers.txt /app/stunservers.txt
COPY nodely_testnet_node.json /app/nodely_testnet_node.json
COPY scripts/docker_start.sh /app/docker_start.sh

# Ensure the start script and binary are executable
RUN chmod +x /app/docker_start.sh /app/bingle_cli

# Default environment variables (can be overridden at runtime)
ENV PASSPHRASE="" \
    EXTERNAL_IP="" \
    PORT=""

# ENTRYPOINT to the startup script which launches /app/bingle_cli run ...
ENTRYPOINT ["/app/docker_start.sh"]
