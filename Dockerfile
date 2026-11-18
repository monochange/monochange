# syntax=docker/dockerfile:1

# ── Builder stage ──────────────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS builder

# The app workspace builds on nightly (edition 2024).
ARG NIGHTLY_TOOLCHAIN=nightly-2026-05-08

# Install build dependencies. binaryen supplies `wasm-opt` for the leptoskit
# release builds (upstream: "sudo apt install binaryen"); jq reads the
# leptoskit CLI pin from [workspace.metadata.bin] so the image never copies
# the revision into this file.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        binaryen \
        brotli \
        jq \
        perl \
        pkg-config \
        fonttools \
        libssl-dev \
        curl \
        ca-certificates \
        xz-utils \
    && rm -rf /var/lib/apt/lists/* \
    && wasm-opt --version

RUN rustup toolchain install ${NIGHTLY_TOOLCHAIN} \
    && rustup default ${NIGHTLY_TOOLCHAIN} \
    && rustup target add wasm32-unknown-unknown

WORKDIR /app

# The root manifests come first so the CLI pin read has its source of truth:
# root `cargo metadata` parses every workspace member, so the crates and the
# docs manifest must exist before it runs.
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY docs/Cargo.toml docs/Cargo.toml
COPY docs/src/ docs/src/

# Build the provenance-verified leptoskit CLI from the SAME Git revision the
# workspaces pin in [workspace.metadata.bin] — never the stale crates.io
# release. `leptoskit generate deploy` cannot export this workspace (the
# root crates monochange_core, monochange_github, and monochange_hosting are
# path dependencies outside the app/ workspace, which the upstream exporter
# refuses to move or vendor), so this hand-maintained recipe follows the
# upstream deployment and runtime conventions with the real schemas instead:
# it is a reviewed deployment, not generated output.
#
# After installing, cargo's own install manifest must name the pinned Git
# revision and own the `leptoskit` binary before anything built with it is
# trusted — the same proof scripts/leptoskit-cli.sh enforces on the host.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/tmp/leptoskit-install \
    set -eu \
    && pin="$(cargo metadata --no-deps --format-version 1 | jq -c '.metadata.bin.leptoskit_cli')" \
    && cli_git="$(printf '%s' "${pin}" | jq -r '.git')" \
    && cli_rev="$(printf '%s' "${pin}" | jq -r '.rev')" \
    && cli_version="$(printf '%s' "${pin}" | jq -r '.version')" \
    && cli_bin="$(printf '%s' "${pin}" | jq -r '.bins[0]')" \
    && printf 'leptoskit CLI pin: leptoskit_cli %s from %s at %s\n' "${cli_version}" "${cli_git}" "${cli_rev}" \
    && cargo install --root /usr/local --git "${cli_git}" --rev "${cli_rev}" --version "${cli_version}" --bin "${cli_bin}" --locked leptoskit_cli \
    && jq -e --arg package "leptoskit_cli" --arg version "${cli_version}" --arg rev "${cli_rev}" --arg bin "${cli_bin}" '(.installs // {}) | to_entries | any((.key | startswith($package + " " + $version + " (git+") and endswith("#" + $rev + ")")) and ((.value.bins // []) | index($bin) != null))' /usr/local/.crates2.json >/dev/null \
    && leptoskit --version

# Install Tailwind CSS v4 standalone binary (Linux glibc). The leptoskit
# tooling plugin compiles the site CSS with it before precompression.
RUN arch="$(dpkg --print-architecture)" \
    && case "$arch" in \
        amd64) tw_arch=x64 ;; \
        arm64) tw_arch=arm64 ;; \
        *) echo "unsupported architecture: $arch" >&2; exit 1 ;; \
    esac \
    && curl -fsSLO "https://github.com/tailwindlabs/tailwindcss/releases/download/v4.2.1/tailwindcss-linux-${tw_arch}" \
    && chmod +x "tailwindcss-linux-${tw_arch}" \
    && mv "tailwindcss-linux-${tw_arch}" /usr/local/bin/tailwindcss \
    && tailwindcss --help

# Pinned Node.js LTS (same major as CI's actions/setup-node) so the leptoskit
# tooling plugin runs here exactly as it does in CI. The plugin is
# dependency-free TypeScript (Node type stripping, no node_modules); verify
# the pinned digest from the official SHASUMS256.txt before extracting.
RUN arch="$(dpkg --print-architecture)" \
    && case "$arch" in \
        amd64) node_arch=x64; node_sha=48cc0e21e048790e54c0cf5749c2725005313d652651c40b7a21feb9697aaf75 ;; \
        arm64) node_arch=arm64; node_sha=eb187837a2230d4047161ac0be008584e16b46f726e9c9490ee35dc6b17d9ba8 ;; \
        *) echo "unsupported architecture: $arch" >&2; exit 1 ;; \
    esac \
    && curl -fsSL "https://nodejs.org/dist/v24.22.2/node-v24.22.2-linux-${node_arch}.tar.xz" -o /tmp/node.tar.xz \
    && printf '%s  /tmp/node.tar.xz\n' "$node_sha" | sha256sum -c - \
    && tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1 --no-same-owner \
    && rm -f /tmp/node.tar.xz \
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

# Copy the remaining workspace files. The leptoskit tooling plugin (on PATH
# as `leptoskit-plugin-monochange-web-assets`) prepares the site CSS,
# split-loader minification, and WASM optimization inside each build's own
# scratch output; publish-web-site.ts stages, validates, and promotes the
# assembled site after both builds.
COPY app/ app/
COPY scripts/leptoskit-plugin/ scripts/leptoskit-plugin/
COPY scripts/minify-split-loader.ts scripts/minify-split-loader.ts
RUN chmod +x scripts/leptoskit-plugin/leptoskit-plugin-monochange-web-assets \
    && ln -s /app/scripts/leptoskit-plugin/leptoskit-plugin-monochange-web-assets /usr/local/bin/leptoskit-plugin-monochange-web-assets \
    && printf '%s' '{"protocol":1,"plugin":"monochange-web-assets","hook":"describe","context":{}}' \
        | leptoskit-plugin-monochange-web-assets \
        | grep '"ok":true'

# Build both web frontends with the provenance-verified leptoskit CLI, then
# publish one complete deployment tree.
#
# Each build compiles into a fresh scratch directory under
# .monochange/local/web-build (inside this layer, outside the served tree) so
# the SDK's bundle report and budget check measure the real scratch bytes and
# a rejected build can never have touched the deployment. The publisher runs
# only after BOTH builds succeeded: it stages pkg/site + pkg/projects plus
# the public assets and fonts from the scratch outputs alone, precompresses
# the staged tree, validates it, and publishes it inside this invocation's
# run root. The publication lease stays outside the compiler cache mount,
# so a cancelled image build cannot poison a later build's destination.
# The cargo registry and target
# directory live in BuildKit cache mounts so dependency artifacts survive
# between image builds; anything the final image needs is copied out of the
# mounts because their contents are not part of the layer. The server binary
# is stripped here rather than through a cargo profile, which would also
# strip proc-macro dylibs and corrupt them on macOS hosts.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/app/target \
    set -eu \
    && leptoskit doctor \
    && run_root=".monochange/local/web-build/docker-$(date +%Y%m%d-%H%M%S)" \
    && mkdir -p "$run_root/web" "$run_root/projects-web" \
    && (cd app && MONOCHANGE_ESBUILD_BIN=/usr/local/bin/esbuild leptoskit build --release --platform web --out "/app/$run_root/web") \
    && (cd app/projects && MONOCHANGE_ESBUILD_BIN=/usr/local/bin/esbuild leptoskit build --release --platform web --out "/app/$run_root/projects-web") \
    && node scripts/leptoskit-plugin/publish-web-site.ts --web-root "$run_root" --site-root "$run_root/site" \
    && strip app/target/release/monochange_app \
    && cp -a "$run_root/site" /site \
    && cp app/target/release/monochange_app /monochange_app \
    && test -f /site/pkg/site/monochange_app.js \
    && test -f /site/pkg/site/monochange_app_bg.wasm \
    && test -f /site/pkg/site/monochange_app.css \
    && test -f /site/pkg/site/__wasm_split_manifest.json \
    && test -f /site/pkg/projects/monochange_projects_web.js \
    && test -f /site/pkg/projects/monochange_projects_web_bg.wasm \
    && test -f /site/pkg/projects/__wasm_split_manifest.json \
    && test ! -e /site/pkg/projects/monochange_app.css \
    && test -f /site/fonts/bricolage-800.woff2 \
    && test -f /site/fonts/hanken-400.woff2 \
    && test -f /site/fonts/hanken-700.woff2 \
    && test -f /site/favicon.svg \
    && test -f /site/branding/social.png \
    && test -f /site/pkg/site/monochange_app.js.br \
    && test -f /site/pkg/site/monochange_app.js.gz \
    && test -f /site/pkg/projects/monochange_projects_web_bg.wasm.br \
    && test -f /site/pkg/projects/monochange_projects_web_bg.wasm.gz

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
