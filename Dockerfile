# Multi-stage Dockerfile to build and run the `cli` binary on Amazon Linux (ARM64)
# Build arguments enable multi-arch builds with Docker Buildx
ARG BUILDPLATFORM
ARG TARGETPLATFORM

# -----------------------
# Build stage
# -----------------------
FROM --platform=$TARGETPLATFORM public.ecr.aws/amazonlinux/amazonlinux:2023 AS build

# Install build dependencies
# - gcc, make, pkgconfig, openssl-devel: to compile OpenSSL-dependent crates
# - git, curl: to fetch rustup and potential git deps
# - which: convenience
RUN dnf install -y \
    gcc \
    make \
    pkgconfig \
    openssl-devel \
    git \
    curl \
    which \
    ca-certificates \
  && dnf clean all

# Install Rust toolchain via rustup (stable)
ENV RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH=/opt/cargo/bin:$PATH
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable \
 && rustc -V && cargo -V

# Create workspace directory and copy sources
WORKDIR /workspace
COPY . .

# Build release binary (let Buildx handle target architecture)
RUN cargo build --release --bin cli

# -----------------------
# Runtime stage
# -----------------------
FROM --platform=$TARGETPLATFORM public.ecr.aws/amazonlinux/amazonlinux:2023 AS runtime

# Install runtime dependencies (OpenSSL for reqwest/openssl crates, CA certs)
RUN dnf install -y \
    ca-certificates \
    openssl \
    tzdata \
  && dnf clean all \
  && update-ca-trust

# App directory
WORKDIR /app

# Copy the compiled binary and startup assets
COPY --from=build /workspace/target/release/cli /app/cli
COPY stunservers.txt /app/stunservers.txt
COPY scripts/docker_start.sh /app/docker_start.sh

# Ensure the start script is executable
RUN chmod +x /app/docker_start.sh /app/cli

# Default environment variables (can be overridden at runtime)
ENV PASSPHRASE="" \
    EXTERNAL_IP="" \
    PORT=""

# Expose the port if provided (no-op if unset). Users should publish with -p.
# EXPOSE is optional since PORT is dynamic; leaving commented to avoid invalid value.
# EXPOSE ${PORT}

ENTRYPOINT ["/app/docker_start.sh"]
