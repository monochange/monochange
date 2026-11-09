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

### 2. Shared client contract (implemented 2026-11-09, consumers pending)

- [x] Extract the session handoff DTOs into `app/crates/monochange_shared` (`client_auth.rs`: `ClientKind`, `ClientTokenRequest`, `ClientSession`, `ClientSessionInfo`, `ConnectPreview`; PKCE helpers behind the `client` feature). The existing project/release DTOs were already shared; the platform-neutral `ApiClient`/`Transport` in `api.rs` covers session, organizations, project, website releases, connect URL, exchange, and revoke.
- [x] Define the session handoff protocol and its threat model (see the implementation section below): browser approve → one-time code → verifier exchange → scoped bearer; replay, forge, burn, origin, and epoch cases are tested.
- [ ] Wire `app/native` and `extension` to the shared `ApiClient`; extension storage and the native secure store keep only their own scoped credential. This is the next unit; nothing in `app/native/**` or `extension/**` changed here.

  Remaining contract for that platform unit, fixed by this change and tested server-side: an extension's token exchange executes from its own runtime origin, which the browser sets and scripts cannot forge — `chrome-extension://<id>` (stable per extension) or `moz-extension://<uuid>` (random per Firefox install) — and the token endpoint accepts exactly those two schemes plus a missing `Origin` (native guests), rejecting every web origin without burning the code. The extension manifest needs the API origin in `host_permissions` to read the exchange response cross-origin; the registered `https` redirect (`https://<id>.chromiumapp.org/` on Chrome, or the configured Firefox equivalent) is only the loopback address `identity.launchWebAuthFlow` captures the code at, never a request origin. No `webOrigin`/`origin` permission is granted or trusted for this flow.

### 3. Identity service boundary (resolver mounted; deployable decision still open)

- [ ] Decide whether the existing app remains the identity service or a separate deployable owns it. Keep the decision reversible.
- [x] The authorization funnel is one resolver on the existing SQLite database: `users` stay keyed on the stable GitHub ID, the existing GitHub token vault and OAuth callback are untouched, and no second accounts/identities tables were added. `monochange_app/server_fns` (get_session, signed_in_user_id), the embedded iframe check, the new browser-only session functions, and the bearer gate all resolve through `monochange_app_api::sessions`.
- [x] The host-only secure cookie keeps its name and attributes for web; non-browser surfaces use hashed opaque client tokens bound to the same `users.id` and `session_epoch`.

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

## Client session handoff implementation status (2026-11-09)

Implemented in this change (unit 2 of the work plan, on top of the upstream migration):

- **Store** (`monochange_app_db::client_sessions`): additive migration `007_create_client_sessions` adds `users.session_epoch INTEGER NOT NULL DEFAULT 0`, `client_grants` (code-hash primary key, user FK with cascade, client kind, exact redirect, S256 challenge, epoch, 120 s expiry) and `client_sessions` (id, user FK, unique token hash, client, epoch, created/last-used/expiry, 30-day tokens, hourly `last_used_at` touch). Only SHA-256 hashes are stored. Consumption is a single `DELETE ... RETURNING`; global revocation is one transaction that increments the epoch and deletes the user's grants and sessions; expiry pruning is bounded.
- **Wire contracts** (`monochange_shared`, `client` feature): serde DTOs with `deny_unknown_fields` on the token request and redacting `Debug` for every secret-bearing type; `pkce_pair` (RFC 7636 verifier + S256 challenge), syntax bounds for verifier/challenge/state; a `Transport` trait and `ApiClient` using the real generated server-function paths (`/api/get_session`, `/api/list_organizations`, `/api/project_overview`, `/api/get_website_releases`) and the JSON handoff endpoints. `Transport` uses native async methods with no `Send` future requirement, so wasm and extension transports that hold `JsValue` handles across `await` can implement it (`api_tests::a_transport_future_may_hold_rc_across_await` pins an `Rc`-across-`await` transport through the client). The `ApiClient` owns no credential state: `with_bearer` restores one from the platform's secure store, `exchange` returns the new session for the caller to persist and sends no `Authorization` header even when the client carries a stale bearer, and `revoke` acts on the caller-supplied bearer. No Axum or database types leak into the shared crate.
- **Resolver** (`monochange_app_api::sessions`): `resolve_parts` maps an `Authorization: Bearer mcd_...` header to a `Client` principal by token-hash join against `users`, and the host-only cookie to a `Browser` principal by JWT signature, expiry, stable `github_id`, and `session_epoch` match. A present bearer never falls back to a cookie. A forged or expired credential is `Invalid`; a well-signed credential whose user row or epoch no longer matches is `Stale`, so a stale browser session reads as signed out (`get_session`) while `signed_in_user_id` still rejects it — preserving the pinned behavior of the dashboard, installation, and repos flows. `principal_middleware` resolves every request once into request extensions — SSR pages, API calls, and the handoff alike — so a signed-in cookie renders `GET /dashboard` signed in through the same resolver; server functions fall back to resolving their own request parts only when they run outside that middleware. A resolver failure (for example a database outage) answers 503 itself, in the Leptos server-function error encoding on server-function paths, and downstream authorization never runs — an outage must not masquerade as an anonymous or invalid session.
- **Browser handoff** (`monochange_app_api::client_auth`, `monochange_app::server_fns::session`): `POST /connect/approve` requires a browser principal and the site's own `Origin` (derived from the configured GitHub callback origin, never a user-supplied host header), validates the exact registered redirect, S256 challenge (43 b64url chars), and 16–128 safe state, then 303-redirects with only `code` and `state`. `POST /api/client-sessions/token` burns the code before validating the verifier, client, redirect, binding, expiry, and epoch, so a wrong guess can never leave a code reusable; only a web `Origin` is rejected without burning so a forged form post cannot destroy a code the real client holds the verifier for. Responses are always `Cache-Control: no-store` under a 4 KiB body limit. Native redirects reuse the upstream `leptoskit_plugin_web_auth::return_address` shape (`dev.monochange.app:/auth/native/return` by default); extension redirects are exact configured `https` URIs, empty by default, never wildcarded. The token endpoint accepts a missing `Origin` (native guests send none) and the extension runtime origins `chrome-extension://<id>` / `moz-extension://<uuid>` — the origin the browser sets for an extension's own fetch, which scripts cannot forge and whose per-install values no allowlist can predict; every web origin, including the registered redirect's origin (only the loopback address the code is captured at), is rejected without burning.
- **Access split**: new browser-only server functions `connect_request`, `list_client_sessions`, `revoke_client_session`, `sign_out_everywhere` (state-changing ones guarded by same-origin `Origin`); a `bearer_gate` middleware mounted after the principal middleware allows client tokens only onto `get_session`, `list_organizations`, `organization_overview`, `project_overview`, `get_website_releases` (generated `ServerFn::PATH` constants), the revoke endpoint, and nothing else. A presented bearer that failed resolution answers 401 on the server-function surface (in the Leptos error encoding) instead of each function's generic error, and the resolver already guaranteed it never fell back to the cookie behind it; a forged cookie (no `Authorization` header) keeps reaching the server functions so the pinned browser signed-out UX is unchanged. Ordinary logout is unchanged; `sign_out_everywhere` bumps the epoch, revokes every grant and client session, and clears the cookie. Installed clients hold their own revocable credential for the same account; this change does not claim that signing in on one device signs in another.

Threat tests (acceptance mapping, all in the app workspace suites):

- A) forged/wrong/expired/replayed grants, wrong verifier/client/redirect burns, epoch drift, pre-signout grant fails — `client_auth_tests::{a_wrong_verifier_burns_the_code_even_for_a_later_correct_attempt, wrong_client_or_redirect_binding_burns_the_code, expired_and_replayed_grants_are_rejected, an_epoch_drift_after_the_grant_was_issued_fails, a_grant_issued_before_a_global_signout_fails}`, `client_sessions_tests::{grants_are_single_use, expired_rows_are_pruned, global_revocation_invalidates_grants_sessions_and_the_epoch}`.
- B) unauthenticated/bearer approval rejected, hostile/missing origin rejected, off-allowlist/unsafe redirect rejected without a code — `client_auth_tests::{approval_requires_a_browser_cookie_and_rejects_a_client_bearer, approval_rejects_missing_and_foreign_origins, approval_rejects_unregistered_redirects_and_malformed_input, redirect_validation_rejects_unsafe_native_and_extension_addresses}`.
- C) consent → exchange → bearer reads the same database identity and only its own rows — `client_auth_tests::a_valid_handoff_exchanges_the_code_for_a_client_session`, `sessions_tests::a_client_token_resolves_to_the_same_user`, `auth_tests::a_client_token_reads_the_session_of_its_own_account`, `installation_tests::a_client_token_sees_only_its_own_account_surface`; existing tenant tests (repos/organizations) pass unchanged with epoch-0 cookies.
- D) expired/revoked/deleted clients denied; cookie+bearer acts bearer with no fallback; non-allowlisted bearer path forbidden — `sessions_tests::{a_bearer_wins_over_a_cookie_and_never_falls_back, expired_revoked_and_deleted_clients_are_invalid, forged_cookies_are_invalid_and_stale_ones_are_stale}`, `session_tests::{client_tokens_cannot_reach_browser_only_endpoints, client_tokens_keep_the_read_only_surface_and_self_revocation}`.
- E) logout preserved; global signout invalidates old JWT and all tokens/grants; legacy epochless JWT valid at epoch 0; new sign-in mints the current epoch — `auth_tests::{logout_expires_the_host_only_session_cookie, signing_in_after_a_global_revocation_mints_the_current_epoch}`, `session_tests::sign_out_everywhere_revokes_every_credential_and_clears_the_cookie`, `sessions_tests::a_legacy_epochless_jwt_is_valid_at_epoch_zero`, `lib_tests::legacy_claims_without_an_epoch_deserialize_as_zero`.
- F) no token in redirect, secret `Debug` redaction, no-store on success and failure, bounded bodies — `client_auth_tests::{approval_redirects_with_only_an_opaque_code_and_state, oversized_handoff_bodies_are_rejected_and_leave_nothing_behind}`, `client_auth_tests::client_session_debug_redacts_the_bearer_token` shape checks live in `monochange_shared` `client_auth_tests::{token_request_denies_unknown_fields_and_redacts_secrets, client_session_debug_redacts_the_bearer_token}`.

Dependency note: adding `leptoskit_plugin_web_auth` from the moving upstream `main` branch resolved to a newer commit on a plain re-resolve. The lockfile was instead resolved with all three leptoskit Git dependencies pinned by `rev`, then converted back to the `branch = main` form at the same revision, and validated with `cargo check --locked --offline`: every `leptoskit*` package still names `a93214edc8e5f6c7999c628285093f15cf188639`.

Verification (2026-11-09, this recovery unit, all exit 0 unless noted):

- `mbx test --locked --manifest-path app/Cargo.toml -p monochange_shared -p monochange_app_db -p monochange_app_api -p monochange_backend --lib` → 4 passed (shared), 47 (app_db), 302 (app_api), 23 (backend).
- `mbx test --locked --manifest-path app/Cargo.toml -p monochange_shared --features client --lib` → 21 passed.
- `mbx test --locked --manifest-path app/Cargo.toml -p monochange_app --lib` → 314 passed, 1 ignored (pre-existing).
- `mbx check --locked --manifest-path app/Cargo.toml -p monochange_app --features ssr` and `--no-default-features --features hydrate --target wasm32-unknown-unknown` and `-p monochange_shared --features client --target wasm32-unknown-unknown` → Finished, no warnings.
- `mbx clippy --locked --manifest-path app/Cargo.toml --workspace --all-targets -- -D warnings` → clean.
- App CI-equivalent coverage: `cargo-llvm-cov llvm-cov --locked --workspace --lcov` in `app/` (nightly-2026-05-08, `CARGO_PROFILE_TEST_DEBUG=0`, CI env), then `scripts/check-patch-coverage.ts` with base `5dfbf72a3` and target 100 → **970/970 executable changed lines covered (100.00%)**.
- `lint:leptoskit` (upstream CLI, all three configs) → no problems.
- `monochange step validate` → workspace validation passed; `monochange step prepare-release --dry-run --format json` → monochange_app 0.5.0 dry run renders.
- `monochange affected --from HEAD~1 --verify` → the handoff changeset covers the one changed package. `monochange affected --from origin/main --verify` additionally reports the root-workspace packages touched by the earlier provenance commits (`@monochange/skill`, `monochange`, `monochange_core`) as uncovered — that range spans the two upstream migration commits and is inherited from that unit, not this one.

Known limits: native guests and the extension still consume nothing of this — wiring them to the shared client, real consent UI, and screen integration are the next unit; `sign_out_everywhere` has no page yet; the extension handoff stays off until its redirect URI is configured; `eval:skill:contract`'s baseline attribution remains unverified from the migration run and was not touched.

## Risks and open questions

- Upstream `main` is a moving branch. It advanced to `f33c3764e2a3836560d9e22f54b6be6595d8ba65` after this change resolved and verified `a93214ed`; the lockfiles and CLI pin keep the reviewed commit until a deliberate `cargo update` and review.
- The upstream CLI is a git install, not a registry binary. A fresh machine needs network access to GitHub, and the install is not covered by a crates.io checksum.
- The identity boundary and callback transport are security-sensitive. Do not start unit 3 without a written handoff threat model and a test plan for state, redirect, replay, and cross-surface session isolation.
- Native/extension platform SDK checks are limited on this machine; hosted CI remains the authority for APK/app/extension artifacts.

## Decision trail

See [leptoskit-projects-split.decisions.tsv](leptoskit-projects-split.decisions.tsv).
