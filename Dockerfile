# Multi-target runtime Dockerfile (no in-container build)
# - base: Common runtime dependencies and assets
# - cli:  Runs the prebuilt bingle_cli binary with docker_start.sh
# - tests: Runs the prebuilt test binary for testnet_user_reaches_endpoint_available and writes results to a mounted host file

# Allow selecting target platform when building multi-arch images
ARG TARGETPLATFORM=linux/arm64

# ------------------------
# Base stage with common deps and assets
# ------------------------
FROM --platform=$TARGETPLATFORM public.ecr.aws/amazonlinux/amazonlinux:2023 AS base

# Install minimal runtime dependencies (bash for scripts, CA certs, OpenSSL if dynamically linked)
RUN dnf install -y \
    bash \
    coreutils \
    ca-certificates \
    openssl \
    tzdata \
  && dnf clean all \
  && update-ca-trust

# App directory
WORKDIR /app

# Copy common runtime assets
COPY stunservers.txt /app/stunservers.txt
COPY nodely_testnet_node.json /app/nodely_testnet_node.json

# Create output directory for test logs (mounted at runtime)
RUN mkdir -p /out
VOLUME ["/out"]

# ------------------------
# CLI stage: package the prebuilt bingle_cli and start script
# ------------------------
FROM base AS cli

# Path to the prebuilt binary within the build context
# Default matches typical cross/zigbuild path; override with --build-arg BIN_PATH=...
ARG BIN_PATH=target/aarch64-unknown-linux-musl/release/bingle_cli

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
    NODE_FILE="/app/nodely_testnet_node.json"

# ENTRYPOINT to the startup script which launches /app/bingle_cli run ...
ENTRYPOINT ["/app/docker_start.sh"]

# ------------------------
# Tests stage: package a prebuilt test binary and a small runner that writes results to /out
# ------------------------
FROM base AS tests

# Path to the prebuilt test binary (must be provided or match your host build). Example:
#   target/aarch64-unknown-linux-musl/debug/deps/api_testnet_endpoint_available-<hash>
ARG TEST_BIN_PATH

# Copy test runner script and the test binary
COPY scripts/docker_run_test.sh /app/run_test.sh
# Force TEST_BIN_PATH to be provided; if not, COPY will fail and prompt the user to pass it.
COPY ${TEST_BIN_PATH} /app/test_bin

RUN chmod +x /app/run_test.sh /app/test_bin

# Environment controlling the test run
# - OUT_FILE: where to write combined test output (mount /out to collect on host)
# - TEST_FILTER: which test to run from the binary
# - TESTNET_USER / TESTNET_PASSPHRASE must be provided at runtime
ENV OUT_FILE="/out/test_results.txt" \
    TEST_FILTER="testnet_user_reaches_endpoint_available"

# The runner will export BINGLE_RUN_TESTNET=1 and execute the test binary with filter
ENTRYPOINT ["/app/run_test.sh"]

# ------------------------
# Default final image is the CLI runtime, to preserve previous behavior
# ------------------------
FROM cli
