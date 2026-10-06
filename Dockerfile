# syntax=docker/dockerfile:1

# ── Builder stage ──────────────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS builder

# The app workspace builds on nightly (edition 2024).
ARG NIGHTLY_TOOLCHAIN=nightly-2026-05-08

# Install build dependencies
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        perl \
        pkg-config \
        libssl-dev \
        curl \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN rustup toolchain install ${NIGHTLY_TOOLCHAIN} \
    && rustup default ${NIGHTLY_TOOLCHAIN} \
    && rustup target add wasm32-unknown-unknown

# Use the official pinned binary so a small deployment host need not compile
# cargo-leptos itself. Verify the release archive before executing its contents.
RUN arch="$(dpkg --print-architecture)" \
    && case "$arch" in \
        amd64) leptos_arch=x86_64; leptos_sha=895cc7ff10702b4f64b3ea5f9f0872e169c870692429650e13a3ae5550b0cdeb ;; \
        arm64) leptos_arch=aarch64; leptos_sha=d45cd862c45628c9bbca43499eb7755e10f166a752a1dbc2ef2b34b3fd6fe622 ;; \
        *) echo "unsupported architecture: $arch" >&2; exit 1 ;; \
    esac \
    && leptos_target="${leptos_arch}-unknown-linux-musl" \
    && curl -fsSL "https://github.com/leptos-rs/cargo-leptos/releases/download/v0.3.6/cargo-leptos-${leptos_target}.tar.gz" -o /tmp/cargo-leptos.tar.gz \
    && echo "${leptos_sha}  /tmp/cargo-leptos.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/cargo-leptos.tar.gz -C /usr/local/bin --strip-components=1 "cargo-leptos-${leptos_target}/cargo-leptos" \
    && rm /tmp/cargo-leptos.tar.gz \
    && cargo leptos --version

# Install Tailwind CSS v4 standalone binary (Linux glibc)
RUN arch="$(dpkg --print-architecture)" \
    && case "$arch" in \
        amd64) tw_arch=x64 ;; \
        arm64) tw_arch=arm64 ;; \
        *) echo "unsupported architecture: $arch" >&2; exit 1 ;; \
    esac \
    && curl -fsSLO "https://github.com/tailwindlabs/tailwindcss/releases/download/v4.0.8/tailwindcss-linux-${tw_arch}" \
    && chmod +x "tailwindcss-linux-${tw_arch}" \
    && mv "tailwindcss-linux-${tw_arch}" /usr/local/bin/tailwindcss \
    && tailwindcss --help

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY app/ app/
COPY docs/src/ docs/src/

# Build the Leptos SSR application
WORKDIR /app/app
RUN cargo leptos build --release

# ── Final stage ────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

ARG OP_VERSION=v2.30.3
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl unzip util-linux \
    && arch="$(dpkg --print-architecture)" \
    && case "$arch" in \
        amd64) op_arch=amd64 ;; \
        arm64) op_arch=arm64 ;; \
        *) echo "unsupported architecture: $arch" >&2; exit 1 ;; \
    esac \
    && curl -fsSL "https://cache.agilebits.com/dist/1P/op2/pkg/${OP_VERSION}/op_linux_${op_arch}_${OP_VERSION}.zip" -o /tmp/op.zip \
    && unzip /tmp/op.zip op -d /usr/local/bin \
    && chmod +x /usr/local/bin/op \
    && rm -f /tmp/op.zip \
    && op --version \
    && apt-get purge -y --auto-remove curl unzip \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1000 app

# Copy static assets and binary from builder
COPY --from=builder /app/app/target/site /app/site
COPY --from=builder /app/app/target/release/monochange_app /app/monochange_app
COPY app/secretspec.toml /app/secretspec.toml
COPY app/deploy/docker-entrypoint.sh /usr/local/bin/monochange-app-entrypoint

# Ensure correct permissions and create the SQLite data directory.
RUN mkdir -p /data \
    && chmod +x /usr/local/bin/monochange-app-entrypoint \
    && chown -R app:app /app /data

# The entrypoint reads the root-only Compose secret and drops privileges before
# starting the application. Local Compose secret mounts preserve host ownership.
USER root
WORKDIR /app

# SecretSpec and the 1Password CLI must use the unprivileged user's home after
# the entrypoint drops privileges, rather than probing root's private config.
ENV HOME=/home/app
ENV SECRETSPEC_PROFILE=development
ENV DATABASE_URL=sqlite:///data/monochange_app.sqlite3
ENV LEPTOS_SITE_ROOT=/app/site
ENV LEPTOS_SITE_PKG_DIR=pkg
ENV LEPTOS_ENV=PROD
ENV RUST_LOG=info
ENV PORT=3000

VOLUME ["/data"]

EXPOSE 3000

ENTRYPOINT ["monochange-app-entrypoint"]
CMD ["./monochange_app"]
