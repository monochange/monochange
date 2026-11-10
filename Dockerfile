# syntax=docker/dockerfile:1

# ── Builder stage ──────────────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS builder

# The app workspace builds on nightly (edition 2024).
ARG NIGHTLY_TOOLCHAIN=nightly-2026-05-08

# Install build dependencies
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        brotli \
        perl \
        pkg-config \
        libssl-dev \
        curl \
        ca-certificates \
        xz-utils \
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

# Pinned Node.js LTS (same major as CI's actions/setup-node) so the release
# minification script runs here exactly as it does in CI. The script itself is
# dependency-free TypeScript (Node type stripping, no node_modules); verify the
# official SHASUMS256.txt before extracting.
RUN arch="$(dpkg --print-architecture)" \
    && case "$arch" in \
        amd64) node_arch=x64; node_sha=48cc0e21e048790e54c0cf5749c2725005313d652651c40b7a21feb9697aaf75 ;; \
        arm64) node_arch=arm64; node_sha=eb187837a2230d4047161ac0be008584e16b46f726e9c9490ee35dc6b17d9ba8 ;; \
        *) echo "unsupported architecture: $arch" >&2; exit 1 ;; \
    esac \
    && curl -fsSL "https://nodejs.org/dist/v24.22.2/node-v24.22.2-linux-${node_arch}.tar.xz" -o /tmp/node.tar.xz \
    && curl -fsSL "https://nodejs.org/dist/v24.22.2/SHASUMS256.txt" -o /tmp/node-shasums.txt \
    && grep " node-v24.22.2-linux-${node_arch}.tar.xz\$" /tmp/node-shasums.txt | sha256sum -c - \
    && tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1 --no-same-owner \
    && rm -f /tmp/node.tar.xz /tmp/node-shasums.txt \
    && node --version

# Pinned esbuild binary from the npm registry package so the wasm-split loader
# is minified by the same minifier version CI resolves from the workspace
# devDependency. Verify the registry integrity digest before extracting.
RUN arch="$(dpkg --print-architecture)" \
    && case "$arch" in \
        amd64) esbuild_arch=linux-x64; esbuild_integrity="sha512-4xTZr1FUmSoQW4XIWmit3tzQrUTZM+N3P0XV8xROKYF50XfI7xeO90+1bZvNwxIufQ9hDQVRJH5YhgPVF8A/HQ==" ;; \
        arm64) esbuild_arch=linux-arm64; esbuild_integrity="sha512-pW4AC0P3it8c7do9MVM4p51FzHzdM/TZrerurgRcHJ2WTa1VQ1CIq18xncfpBJw4ojkiZZrKW2yIBWBP92j6Ug==" ;; \
        *) echo "unsupported architecture: $arch" >&2; exit 1 ;; \
    esac \
    && curl -fsSL "https://registry.npmjs.org/@esbuild/${esbuild_arch}/-/${esbuild_arch}-0.28.2.tgz" -o /tmp/esbuild.tgz \
    && printf '%s  /tmp/esbuild.tgz\n' \
        "$(printf '%s' "${esbuild_integrity#sha512-}" | base64 -d | od -An -tx1 | tr -d ' \n')" \
        | sha512sum -c - \
    && tar -xzf /tmp/esbuild.tgz -C /tmp package/bin/esbuild \
    && mv /tmp/package/bin/esbuild /usr/local/bin/esbuild \
    && rm -rf /tmp/esbuild.tgz /tmp/package \
    && esbuild --version

WORKDIR /app

# Copy workspace files. `docs/Cargo.toml` is required for `cargo metadata` to
# parse the root workspace during split builds, even though the image never
# compiles the docs crate.
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY app/ app/
COPY scripts/minify-split-loader.ts scripts/minify-split-loader.ts
COPY docs/Cargo.toml docs/Cargo.toml
COPY docs/src/ docs/src/

# Build the Leptos SSR application.
#
# The cargo registry and target directory live in BuildKit cache mounts so
# dependency artifacts survive between image builds; anything the final image
# needs is copied out of the mounts because their contents are not part of the
# layer. The server binary is stripped here rather than through a cargo
# profile, which would also strip proc-macro dylibs and corrupt them on macOS
# hosts.
# Precompress the pkg assets: the static file handler serves these `.gz` and
# `.br` siblings to clients that accept them, so the 1-vCPU droplet never
# compresses the WASM chunks at request time.
WORKDIR /app/app
# cargo-leptos `js-minify` covers the main glue only; the wasm-split loader
# keeps its route comments and full identifiers. Minify it before the site is
# copied out so the shipped assets and their precompressed siblings are the
# same bytes CI's size gate measures.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/app/target \
    cargo leptos build --release --split \
    && node /app/scripts/minify-split-loader.ts --site-root target/site \
    && strip target/release/monochange_app \
    && cp -a target/site /site \
    && cp target/release/monochange_app /monochange_app \
    && find /site/pkg -type f \( -name '*.wasm' -o -name '*.js' -o -name '*.css' \) \
        -exec sh -c 'gzip -9 -k "$1" && brotli -q 11 -c "$1" > "$1.br"' _ {} \;

# ── Final stage ────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

# Monosecret's 1Password provider reads items through the `op` CLI, so the
# image bundles it for the production profile.
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
COPY --from=builder /site /app/site
COPY --from=builder /monochange_app /app/monochange_app
COPY app/monosecret.toml /app/monosecret.toml
COPY app/deploy/docker-entrypoint.sh /usr/local/bin/monochange-app-entrypoint

# Ensure correct permissions and create the SQLite data directory.
RUN mkdir -p /data \
    && chmod +x /usr/local/bin/monochange-app-entrypoint \
    && chown -R app:app /app /data

# The entrypoint reads the root-only Compose secret and drops privileges before
# starting the application. Local Compose secret mounts preserve host ownership.
USER root
WORKDIR /app

# Monosecret (its config and audit log) and the 1Password CLI must use the
# unprivileged user's home after the entrypoint drops privileges, rather than
# probing root's private config.
ENV HOME=/home/app
ENV MONOSECRET_PROFILE=development
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
