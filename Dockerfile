# ═══════════════════════════════════════════════════════════════════
#  pk — meta package manager
#  Minimal Alpine image with statically-linked pk binary
#
#  Build:
#    docker build -t pk .
#    docker build -t pk --build-arg FEATURES=jsonrpc .
#    docker build -t pk --build-arg FEATURES="" .          # bare minimum
#
#  Run:
#    docker run --rm pk --version
#    docker run --rm pk inventory
#    docker run --rm -v "$PWD:/work" pk satisfy /work/Needsfile
#
#  With host package managers (useful for actual installs):
#    docker run --rm -it \
#      -v /var/cache/apk:/var/cache/apk \
#      --user root \
#      pk install curl
# ═══════════════════════════════════════════════════════════════════

# ───────────────────────────────────────────────────────────────────
#  Stage 1: Build
# ───────────────────────────────────────────────────────────────────
FROM rust:1.85-alpine3.21 AS builder

# Build-time dependencies:
#   musl-dev      — musl libc headers (static linking)
#   openssl-dev   — TLS for reqwest (Repology, crates.io, AUR, RubyGems)
#   pkgconfig     — locate openssl via pkg-config
#   perl, make    — openssl-sys vendored build fallback
RUN apk add --no-cache \
        musl-dev \
        openssl-dev \
        pkgconfig \
        perl \
        make

WORKDIR /build

# Cache dependencies: copy manifests first, build a dummy main,
# then replace with real sources. Layer is invalidated only when
# Cargo.toml changes, not on every source edit.
COPY Cargo.toml Cargo.lock* ./
COPY crates/ crates/

# Force static linking of OpenSSL so the final binary has zero
# shared-library dependencies (runs on any musl or even scratch).
ENV OPENSSL_STATIC=1 \
    PKG_CONFIG_ALL_STATIC=1 \
    CARGO_NET_GIT_FETCH_WITH_CLI=false \
    CARGO_INCREMENTAL=0

# Feature selection:
#   default  = "telemetry,jsonrpc"  (full binary, ~30 MB)
#   jsonrpc  = RPC server only      (~18 MB)
#   ""       = bare CLI             (~14 MB, fastest build)
ARG FEATURES=""
RUN if [ -z "$FEATURES" ]; then \
        cargo build --release -p rspk-cli --no-default-features; \
    else \
        cargo build --release -p rspk-cli --no-default-features \
            --features "$FEATURES"; \
    fi \
    && strip target/release/pk

# ───────────────────────────────────────────────────────────────────
#  Stage 2: Runtime
# ───────────────────────────────────────────────────────────────────
FROM alpine:3.21

# ca-certificates: required for HTTPS requests to package registries
# (repology.org, crates.io, aur.archlinux.org, rubygems.org).
# tzdata: optional, for correct timestamps in logs.
RUN apk add --no-cache \
        ca-certificates \
        tzdata \
    && adduser -D -H -s /sbin/nologin -g "pk runtime" pk \
    && mkdir -p /work \
    && chown pk:pk /work

# The binary is fully static (musl + static openssl), so no
# additional runtime libraries are needed.
COPY --from=builder --chown=root:root /build/target/release/pk /usr/local/bin/pk

# Metadata
LABEL org.opencontainers.image.title="pk" \
      org.opencontainers.image.description="Meta package manager — universal install script" \
      org.opencontainers.image.url="https://github.com/vi-is-ramen/rspk" \
      org.opencontainers.image.source="https://github.com/vi-is-ramen/rspk" \
      org.opencontainers.image.licenses="MIT"

# Run as non-root by default. Override with --user root when
# actual package installation is needed.
USER pk
WORKDIR /work

# pk is the entrypoint; arguments are passed directly:
#   docker run --rm pk inventory
#   docker run --rm pk --dry-run install ripgrep
ENTRYPOINT ["pk"]
CMD ["--help"]
