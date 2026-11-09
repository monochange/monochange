# leptoskit projects split

Status: dependency migration implemented by this change; surface and identity split pending. Created: 2026-11-09 Baseline: `origin/main` at `9dd2d1d0397b6e7907e5cb08b3939929db008c67`. Upstream: `https://github.com/leptoskit/leptoskit` `main` at `a93214edc8e5f6c7999c628285093f15cf188639` (2026-11-09).

## Goal

Move every monochange workspace to leptoskit directly from the upstream Git repository, and record a versioned plan for splitting the product surfaces:

1. a cross-platform **monochange projects** client (iOS, Android, browser extension, and a web client), and
2. the existing **landing / GitHub connection** website,

backed by one authoritative identity and session service. Users sign in once and every surface sees the same account, organizations, projects, and repository connections.

This change implements only the upstream dependency migration. The split and the authentication work below are marked pending and are not implemented here.

## Non-goals for this change

- No surface split, route split, new domain, DNS change, or production change.
- No new identity provider, session database, or token format.
- No design change to the existing website or extension UI.
- No release, publish, tag, or deployment operation.
- No unrelated dependency upgrades or formatting sweeps.

## Current architecture on origin/main

### Repository shape

- The root workspace is the `monochange` CLI/library monorepo and excludes `app`.
- `app/`, `app/native/`, and `extension/` are three independent Cargo workspaces, each with its own `Cargo.lock` and wasm/native targets.
- `monochange.toml` configures `monochange_app` and `monochange_extension` as private release targets. `app/native/**` is covered by `monochange_app`'s `additional_paths`.

### Web app (`app/`)

- One Leptos SSR binary (`monochange_app`) with a hydrate wasm client built by cargo-leptos. `app/leptoskit.toml` is lint-only; `[package.metadata.leptos]` still owns the build.
- The same binary serves the landing page, login, dashboard, changelog, book, feedback/embed APIs, hosted release APIs, and GitHub App webhooks.
- Identity is GitHub OAuth. `monochange_app_api::oauth` signs a JWT session carried in the host-only `__Host-monochange_session` cookie (`Secure`, `HttpOnly`, `SameSite=Lax`, path `/`, no `Domain`). OAuth state is bound to a second host-only cookie.
- SQLite through `monochange_app_db` stores users, organizations, GitHub App installations, repositories, projects, and feedback. Repository connection is a GitHub App installation owned by the signed-in account.

### Native (`app/native/`)

- A separate workspace with a Leptos 0.9 CSR wasm guest (`monochange_native_app`), shared RPC types and operations in `monochange_rpc`, Android/iOS host crates, and a skeletal `monochange_native` server binary.
- `session_probe` always returns an anonymous handshake and `list_projects` returns one hard-coded demo project. The native guest has no real identity, no cookie, and no access to the website database.
- `monochange_native_app` is checked by default; `android` and `ios` are platform host crates that need SDKs unavailable on this machine.

### Extension (`extension/`)

- A separate leptoskit extension workspace with popup, options, and background surfaces. It stores a bearer string in extension storage and posts to `/rpc/session`, `/rpc/login-url`, `/rpc/logout`, `/rpc/organizations`, and `/rpc/website-releases`.
- Those `/rpc/*` endpoints are not mounted by `app/`, so the extension has no working shared session today. It is a bearer-oriented stub, not a second identity system, but it also cannot reuse the website's host-only cookie.

## Proposed target

- **monochange projects**: a distinct cross-platform client for organizations, projects, releases, and feedback. Share wire types and operations through one Rust crate instead of copying DTOs into native and extension.
- **landing / GitHub connection site**: keeps the public marketing pages, sign-in entry point, dashboard, billing, changelog, docs, and GitHub App installation/connection flows.
- **One identity/session authority**: one service owns users, OAuth identities, sessions, organizations, and installations. Other surfaces call it; they do not create users or sessions of their own.
- **Browser-based handoff**: native apps and the extension open the system browser to the same sign-in page, then receive a short-lived, one-time code over a registered callback or extension redirect. The client exchanges that code for its own scoped session credential. No demo identities, no manually pasted bearer tokens, and no independent auth database.
- **Cookie preservation**: the web session remains a host-only `__Host-...` secure cookie with the current attributes. Native/extension credentials are not cookies and must never be attached to browser requests.
- **Reversible deployment**: keep `monochange.dev` and existing paths for now. A projects subdomain or path is a later, reversible decision; nothing in this plan requires DNS, certificates, or a production migration.

## Work units

### 1. Upstream leptoskit dependency migration (this change)

- Replace every direct `leptoskit*` crates.io dependency in `app/`, `app/native/`, and `extension/` with `git = "https://github.com/leptoskit/leptoskit", branch = "main"`.
- Resolve one upstream commit and commit a lockfile in each workspace that names the same source URL and revision.
- Point the lint/build CLI at the same repository and revision; keep `lint:leptoskit` using that binary rather than a stale crates.io install.
- Apply only compatibility edits required by the new upstream APIs. Stop and report if the migration becomes architectural.

### 2. Shared client contract (pending)

- [ ] Extract the project/release DTOs and RPC operations into one shared crate that web, native, and extension consume.
- [ ] Define the session handoff protocol (authorize URL, callback, one-time code, exchange, refresh, revoke) and its threat model.
- [ ] Define a single credential store abstraction; extension storage and the native secure store hold only the client's own scoped credential.

### 3. Identity service boundary (pending)

- [ ] Decide whether the existing app remains the identity service or a separate deployable owns it. Keep the decision reversible.
- [ ] Mount the shared RPC router beside the existing OAuth routes and reuse `monochange_app_db`; do not add a second user/session table.
- [ ] Keep the host-only secure cookie for web and add scoped credentials for non-browser surfaces.

### 4. Surface split (pending)

- [ ] Split the projects client from the landing/GitHub connection site without duplicating auth code or design tokens.
- [ ] Move native and extension from demo/anonymous data to the shared service.
- [ ] Keep GitHub App installation and repository connection owned by the website, with the projects client reading the same connection state.

## Three verification units

Each unit must pass independently because the repository has three Cargo workspaces and three deployment surfaces.

1. **Web app** (`app/`): library tests, SSR check, hydrate wasm check, and the app's existing test suite. This unit proves the landing site and the identity/session code still compile together.
2. **Native** (`app/native/`): default-member check plus the guest wasm target and the native guest feature check. Platform host crates are verified only as far as this machine's SDKs allow, and that limit is reported.
3. **Extension** (`extension/`): default workspace check, extension wasm check, `leptoskit lint` with the upstream CLI, and the extension tests. This unit proves the popup/options/background surfaces still build and lint.

## Validation commands

Exact commands and results are recorded in the decision trail. The baseline sequence is:

```bash
devenv shell mbx test --manifest-path app/Cargo.toml -p monochange_shared -p monochange_backend --lib
devenv shell mbx check --manifest-path app/Cargo.toml -p monochange_app --features ssr
devenv shell mbx check --manifest-path app/Cargo.toml -p monochange_app --no-default-features --features hydrate --target wasm32-unknown-unknown
devenv shell mbx check --manifest-path app/native/Cargo.toml
devenv shell mbx check --manifest-path extension/Cargo.toml
```

Then run `fix:all`, `lint:all`, the required tests, `monochange step
validate`, the release preview, and affected-package checks. Run `coverage:patch` if executable lines change.

## Implementation and verification status (2026-11-09)

Implemented in this change:

- every direct `leptoskit*` dependency in `app/`, `app/native/`, and `extension/` points at `https://github.com/leptoskit/leptoskit` on `main`;
- all three lockfiles name the same reviewed revision, `a93214edc8e5f6c7999c628285093f15cf188639`, and no registry copy remains;
- `[workspace.metadata.bin]` pins `leptoskit_cli` to that revision with the `leptoskit` binary target. `lint:leptoskit` reads that entry, verifies every `leptoskit*` source in the three lockfiles against the pinned Git revision, and reuses a cached `.bin` install only when cargo's `.crates.toml`/`.crates2.json` records that Git source for the `leptoskit` binary. A same-version registry install or a different revision is replaced with a forced `cargo install` of the pinned revision.

The migration run reported: SSR and hydrate wasm checks, 797 app tests, native default/client wasm/native guest checks, extension default/extension wasm checks, 10 extension tests, upstream `leptoskit lint` over all three configs, `lint:all`, `test:cargo` (4469/4469), documentation tests, Node tests, configuration validation, release preview (app 0.5.0, extension 0.1.2), notes rendering, and affected-package coverage all passed. The correction re-ran `devenv shell mbx test --locked --manifest-path app/Cargo.toml -p monochange_shared -p monochange_backend --lib`; its 8 tests passed with command-scoped `CC=/usr/bin/clang CXX=/usr/bin/clang++ CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/usr/bin/clang` and the mbx Cargo shim on `PATH`. The broader 797-test app claim was not re-run in the correction. `eval:skill:contract` failed its `init-provider-workflows` scenario in the migration run; baseline attribution at `9dd2d1d03` is unverified, and this diff does not touch that CLI behavior. The full command trail is in the decision log.

Local note: the Nix Apple SDK 14.4 linker rejects host SDK `.tbd` files (`unknown architecture arm64e.x1-macos`) when the default Nix linker is selected. The migration run reported `LIBRARY_PATH=/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/lib` plus the mbx shim; the correction re-check used the command-scoped host clang variables above instead and passed. Neither workaround is a project change.

### Correction: CLI cache provenance

The first implementation selected the managed CLI with `find .bin ... | sort | tail`. `cargo-run-bin` 1.7.5 keys install directories by toolchain, crate, and version, so a cached crates.io 0.3.0 install occupies the same path as the pinned Git install and survives a broad CI cache restore. The corrected `lint:leptoskit` derives the Git URL, revision, version, and binary name from `[workspace.metadata.bin]`, rejects any `leptoskit*` lockfile source that does not end in the full pinned revision, and trusts a cached binary only when its install manifest names the pinned Git source and owns that binary. Otherwise it force-installs only `leptoskit_cli` from the pinned revision. A real registry-source manifest at the cached path was replaced by that install and the lint task then passed over all three configs; the focused tooling tests cover the rejection, reinstall, reuse, drift, and install-failure branches.

## Risks and open questions

- Upstream `main` is a moving branch. It advanced to `f33c3764e2a3836560d9e22f54b6be6595d8ba65` after this change resolved and verified `a93214ed`; the lockfiles and CLI pin keep the reviewed commit until a deliberate `cargo update` and review.
- The upstream CLI is a git install, not a registry binary. A fresh machine needs network access to GitHub, and the install is not covered by a crates.io checksum.
- The identity boundary and callback transport are security-sensitive. Do not start unit 3 without a written handoff threat model and a test plan for state, redirect, replay, and cross-surface session isolation.
- Native/extension platform SDK checks are limited on this machine; hosted CI remains the authority for APK/app/extension artifacts.

## Decision trail

See [leptoskit-projects-split.decisions.tsv](leptoskit-projects-split.decisions.tsv).
