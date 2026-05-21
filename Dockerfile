# syntax=docker/dockerfile:1.7
#
# Slab Server — self-hosted PDF toolkit. Same Rust core as the Tauri
# desktop app, behind a small axum HTTP API + drag-drop web UI.
#
# Multi-stage build:
#   1. `chef` (cargo-chef)   → cache crate dependency layer
#   2. `planner` / `builder` → compile slab-server in release mode
#   3. `runtime`             → distroless-style debian:slim with the
#                              binary + embedded UI, ~80MB total.
#
# Build:  docker build -t ghcr.io/sanjays2402/slab:latest .
# Run:    docker run --rm -p 8080:8080 ghcr.io/sanjays2402/slab:latest
# Open:   http://localhost:8080

ARG RUST_VERSION=1.83
ARG DEBIAN_RELEASE=bookworm-slim

# ────────────────────────────────────────────────────────────────────
# Stage 1 — base toolchain (cargo-chef for dependency caching)
# ────────────────────────────────────────────────────────────────────
FROM rust:${RUST_VERSION}-${DEBIAN_RELEASE} AS chef
RUN cargo install cargo-chef --locked --version 0.1.71
WORKDIR /build

# ────────────────────────────────────────────────────────────────────
# Stage 2 — recipe (only needs Cargo.toml/Cargo.lock)
# ────────────────────────────────────────────────────────────────────
FROM chef AS planner
COPY src-tauri/Cargo.toml src-tauri/Cargo.lock ./src-tauri/
# Stub the workspace so cargo-chef can resolve the manifest without
# needing the actual sources. We replace it with the real tree in
# the builder stage below.
RUN mkdir -p src-tauri/src/bin && \
    echo 'fn main() {}' > src-tauri/src/bin/server.rs && \
    echo 'fn main() {}' > src-tauri/src/bin/slab.rs && \
    echo 'fn main() {}' > src-tauri/src/bin/sign_plugin.rs && \
    echo '' > src-tauri/src/lib.rs && \
    cp src-tauri/Cargo.toml Cargo.toml && \
    cargo chef prepare --recipe-path recipe.json --bin slab-server || true

# ────────────────────────────────────────────────────────────────────
# Stage 3 — build (cook deps, then compile slab-server)
# ────────────────────────────────────────────────────────────────────
FROM chef AS builder
# System libraries required by the Tauri-aware crates we transitively
# pull in. `pkg-config` + `libssl-dev` ride along with the rustls-tls
# reqwest build for `oid_registry` & friends; cleaning them isn't worth
# the rebuild noise. `build-essential` is here for cc/ld toolchains.
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libssl-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Bring the actual workspace in. We don't use cargo-chef's recipe path
# here because the workspace pulls in tauri-build, which insists on
# the full source tree; the chef stage is best-effort and the second
# `cargo build` benefits from the Docker layer cache instead.
COPY . .

# Build slab-server with the `server` feature in release mode.
# The `target` mount caches the compiler output across rebuilds — without
# it a fresh build is ~6 min, with it incremental builds drop to <30s.
RUN --mount=type=cache,target=/build/src-tauri/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cd src-tauri && \
    cargo build --release --bin slab-server --features server && \
    mkdir -p /out && \
    cp target/release/slab-server /out/slab-server

# ────────────────────────────────────────────────────────────────────
# Stage 4 — runtime (debian:slim, no Rust toolchain, ~80MB)
# ────────────────────────────────────────────────────────────────────
FROM debian:${DEBIAN_RELEASE} AS runtime

# `libssl3` for reqwest TLS, `ca-certificates` for HTTPS verification
# (Beacon AI features may make outbound HTTPS calls when configured).
# Tini is the PID-1 init so SIGTERM reaches axum cleanly on `docker stop`.
RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl3 \
        ca-certificates \
        tini \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd -r slab && useradd -r -g slab -d /var/lib/slab -m slab

COPY --from=builder /out/slab-server /usr/local/bin/slab-server

# Sane defaults — override with `-e` on `docker run`.
ENV SLAB_BIND=0.0.0.0:8080 \
    SLAB_MAX_UPLOAD_MB=256 \
    SLAB_DATA_DIR=/var/lib/slab \
    RUST_LOG=info

USER slab
WORKDIR /var/lib/slab
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD ["sh", "-c", "wget -qO- http://localhost:8080/healthz || exit 1"]

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/slab-server"]
