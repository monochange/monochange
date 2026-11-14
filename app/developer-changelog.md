# Developer updates for monochange.dev

Release notes for website developers and operators.

## 0.5.0

### 💥 Breaking Change

#### Build each web frontend with the pinned leptoskit CLI and assemble one site

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)

The website and projects browser entries now build through their own `leptoskit.toml` with the provenance-verified `leptoskit` CLI, replacing the cargo-leptos pipeline in CI and the deployment image. Both configs name the shared `monochange_app` backend and select their own browser package, and `leptoskit.workspace.toml` at the repository root declares website (landing), projects (product), native (product), and extension (product).

The projects browser package is the dedicated `monochange_projects_web` cdylib, which owns the product route tree (`#[lazy_route]` pages) and the wasm-bindgen `hydrate` export; `monochange_projects` stays the shared product library (components, server functions, embedded feedback) with no cdylib and no `web`/`native` feature pair. The split is load-bearing for size, not naming: `#[lazy_route]` emits `#[no_mangle]` wasm-split exports in the crate that declares them, and those exports are linker roots in every browser bundle that compiles the crate. With the routes in the shared library, the website bundle retained the whole product route code (~824 KiB raw) as unreachable-but-exported functions; with the dedicated entry crate, the website client carries none of it. The website backend still renders the product route tree server-side through a host-only dependency on the entry crate, and the projects bundle loads `/pkg/projects/monochange_projects_web.js`.

Before (CI and Docker):

```bash
cargo leptos --manifest-path crates/monochange_app/Cargo.toml build --release --split
pnpm node scripts/minify-split-loader.ts --site-root app/target/site
find /site/pkg -type f \( -name '*.wasm' -o -name '*.js' -o -name '*.css' \) \
  -exec sh -c 'gzip -9 -k "$1" && brotli -q 11 -c "$1" > "$1.br"' _ {} \;
```

After:

```bash
# Resolves the CLI from [workspace.metadata.bin] and verifies the Git pin,
# builds both frontends into fresh scratch outputs under
# .monochange/local/web-build/<run>/, and only after both builds pass their
# own bundle reports and budgets publishes one validated tree at
# app/target/site. Docker and CI run the same build-then-publish sequence.
devenv shell build:web --release
```

The tooling plugin declared under `[[plugins]]` in both web configs runs inside each build, before release precompression: it compiles the shared Tailwind input to the entry CSS (the leptoskit build produced no CSS on its own), minifies the wasm-split loader and each entry's wasm-bindgen glue through the guarded `scripts/minify-split-loader.ts` (restoring cargo-leptos `js-minify` parity for the main `{output_name}.js`), and leaves each frontend's finished bundle in its own scratch output. After both builds succeed, `scripts/leptoskit-plugin/publish-web-site.ts` assembles the bundles under `pkg/site/` and `pkg/projects/`, so both entries share one deployment without colliding split manifests or chunk names, and promotes the validated tree. Missing tools fail the build loudly; run `leptoskit doctor` to check a build environment. The plugin resolves as `leptoskit-plugin-monochange-web-assets` on PATH (the devenv profile provides it; provide the same symlink in other environments).

The server composes both SSR route sets in one registration — `leptos_routes_with_context` receives the merged route listings of both trees, so `/api` server functions register exactly once — and renders the entry scripts, stylesheet, and split manifest of the frontend that owns the request path. leptos 0.9's `HydrationScripts` caches entry names process-globally, so the composition renders per-frontend hydration scripts explicitly; the emitted markup matches upstream for each entry. Because the router intercepts same-origin anchor clicks, links that cross between the website and projects entries render with `target="_self"`, forcing the real document navigation that loads the other bundle.

The committed size gate (`scripts/website-size.ts check`) uses the maintainer-approved two-entry baseline, including the deployed WOFF2 fonts. Named assets retain individual 5% guards, anonymous split chunks have a 5% aggregate guard within each namespace, and the combined transfer total retains its 1% tolerance. Route-hash changes preserve comparisons; missing or ambiguous assets fail instead of being skipped. Compatibility mappings also support older flat baselines. The raw benchmark budgets (`scripts/benchmark-website.ts`) no longer count precompressed `.br`/`.gz` twins, and every frontend's eager entry wasm is checked against the eager budget by name.

The Docker image builds the pinned CLI from the same Git revision instead of installing a release archive, installs binaryen for `wasm-opt`, and keeps the Monosecret privilege drop, no-new-privileges, SQLite volume, secret mounts, `/health`, and Class B APIs unchanged. `scripts/lint-leptoskit.sh` now resolves the CLI through the shared `scripts/leptoskit-cli.sh` helper, so lint, build, and workspace commands use one pin implementation.

#### Separate projects from repository connection and share client sessions

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #886](https://github.com/monochange/monochange/pull/886)

> Rebuild native hosts for the new sign-in plugins and migrate product links to `/projects`.

Move the browser workspace into `monochange_projects` at `/projects`, with its own navigation. `/dashboard` retains GitHub repository connection; existing organization, project and feedback links redirect into projects. Mount `/connect` and `/account` for client consent and revocation against the existing cookie authority. `connect_request` returns the validated `denial_url`; approval forms submit only `client`, `redirect_uri`, `code_challenge`, and `state`.

```text
Before: /dashboard/acme/projects/widget
After:  /projects/acme/projects/widget

Repository connection: /dashboard
```

Native and extension consumers use `monochange_shared::ApiClient` rather than demo RPC endpoints. Native plugin declarations add crypto, web-auth, and linking alongside HTTP and secure-store: rebuild native hosts before distributing this update. This requires a new native binary, not an OTA-only update.

Register the exact URL returned by the installed extension's `identity.getRedirectURL()` in `MONOCHANGE_EXTENSION_REDIRECT_URIS` before testing or distributing extension login. The Firefox ID remains configured; Chromium unpacked IDs depend on the installation. There is no wildcard callback registration.

### 🚀 Feature

#### Add the shared client-session handoff and one authorization resolver

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #885](https://github.com/monochange/monochange/pull/885)

Every authorization path in the web app now funnels through one resolver (`monochange_app_api::sessions`), which distinguishes a browser cookie principal from an opaque client-token principal and never falls back to a cookie when an `Authorization` header is present. The principal middleware resolves every request once — SSR pages and API calls alike, so a signed-in cookie renders the dashboard signed in — and answers 503 (in the Leptos server-function error encoding on server-function paths) when the resolver itself fails, instead of reading a database outage as an anonymous or invalid session.

New additive schema in `monochange_app_db`: `users.session_epoch` (default 0) plus `client_grants` (hashed one-time codes bound to user, client, exact redirect, PKCE challenge, and epoch) and `client_sessions` (hashed opaque `mcd_` tokens). No secrets are stored, only SHA-256 hashes; codes are consumed by an atomic `DELETE ... RETURNING`, so a wrong verifier, client, or redirect still burns the code.

Browser handoff: `POST /connect/approve` accepts a browser-only, same-origin form post, validates the exact registered redirect and PKCE input, and 303-redirects with only an opaque code and state. `POST /api/client-sessions/token` exchanges code + verifier for a scoped client session; `POST /api/client-sessions/revoke` self-revokes idempotently. All handoff responses, including failures, are `Cache-Control: no-store` with a 4 KiB body cap. The token endpoint accepts a missing `Origin` (native guests) and the extension runtime origins `chrome-extension://` / `moz-extension://`; every web origin — including the registered `https` redirect's origin, which is only the loopback capture address — is rejected without burning the code.

New browser-only server functions: `connect_request` (consent preview), `list_client_sessions`, `revoke_client_session`, and `sign_out_everywhere`, the last of which increments the user's epoch, deletes all their grants and client sessions, and clears the host-only cookie. State-changing session calls require the site's own `Origin`.

Device clients (native/extension) may call `get_session`, `list_organizations`, `organization_overview`, `project_overview`, and `get_website_releases` with their bearer token, and revoke themselves; every other server function is browser-only (403). A presented bearer that fails verification answers 401 on the server-function surface and never falls back to the cookie behind it; a forged cookie keeps the existing browser signed-out error. Existing ordinary logout behavior is unchanged; sign-in now mints the browser JWT at the user's current epoch. JWTs issued before this change keep working until the first `sign_out_everywhere` bumps the epoch.

The shared `monochange_shared` client (`client` feature) is credential-stateless: `ApiClient::with_bearer` restores a credential from the platform's secure store, `exchange` returns the new `ClientSession` for the caller to persist (and sends no `Authorization` header even when the client carries a stale bearer), and `revoke` acts on the caller-supplied bearer. Its `Transport` trait uses native async methods without a `Send` future requirement, so wasm and extension transports that hold `JsValue` handles across `await` can implement it.

Redirect allowlists are plain public environment config resolved once at startup, following the existing `AppState` pattern: `MONOCHANGE_NATIVE_REDIRECT_URIS` (default `dev.monochange.app:/auth/native/return`, validated by the upstream `leptoskit_plugin_web_auth` return-address helper) and `MONOCHANGE_EXTENSION_REDIRECT_URIS` (empty by default, so no extension can connect until its exact `https` redirect is configured).

#### Dispatch GitHub Actions workflows through the monochange app

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #866](https://github.com/monochange/monochange/pull/866)

`monochange gh workflow run` dispatches a workflow in a connected repository without a personal access token. The command mirrors the useful part of `gh workflow run`:

```bash
monochange gh workflow run release.yml --repo monochange/actions --ref main
monochange gh workflow run release-pr-merge.yml --repo monochange/actions -f pull_request=103
```

A GitHub Actions job authenticates with its OIDC token by granting `id-token: write`; local runs use `--hosted-auth token` and a deployment `MONOCHANGE_TOKEN`. The monochange app checks that an OIDC caller and the target repository belong to the same GitHub App installation, so an organisation repository can dispatch a sibling repository's release workflow while other installations remain isolated. `--dry-run` validates the target without starting a run, and `--field KEY=VALUE` passes workflow inputs.

The new `HostedWorkflowDispatchRequest` and `HostedWorkflowDispatchResponse` types expose the same contract to other clients. `publish.yml` now uses the command to refresh the `monochange/actions` release PR after a CLI release, replacing the `MONOCHANGE_ACTIONS_TOKEN` credential.

### 🐛 Fixed

- **Fix CI web build tool preparation and scratch paths.** The benchmark job now installs Tailwind before recording build-tool provenance, resolves the workspace-pinned esbuild through `pnpm exec`, and uses one repository-rooted `.monochange/local/web-build` scratch directory for both frontend builds and publication. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Reduce compressed browser module size.** Release web builds now remove compiler metadata and compare additional Binaryen optimizations before assembling and precompressing both browser entries. Each candidate must preserve the module's imports and exports, reduce the size gate's gzip transfer, and avoid increasing deployed gzip or brotli bytes. Invalid output and compiler failures stop the build. Rebuild both entries through the existing leptoskit web-assets plugin; the already-required `wasm-opt` supplies the optimization passes. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Explain configured IDs for same-name ecosystem packages.** The discovery guide now shows distinct configured IDs for packages that share a published name and explains configured-ID precedence over published-name fallback. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #889](https://github.com/monochange/monochange/pull/889)

#### Defer public document hydration and preserve button clicks

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)

The server selects deferred hydration for the public website routes and the signed-out projects landing page. The loader starts on interaction or an idle fallback after load, queues clicks on buttons outside forms, and awaits the browser entry's async hydration export before replaying those clicks in order. Links, native details, and form submissions retain their browser defaults. Signed-in projects and editable product pages continue hydrating eagerly.

Deploy the server and both browser entries together: the loader now awaits `hydrate_from_async()` so lazy routes attach before replay. Deferred documents omit entry and lazy-chunk preloads to keep WASM work off the initial rendering path.

#### Make manual and CI website rollouts one data-preserving serialized transaction

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #894](https://github.com/monochange/monochange/pull/894)

`monochange-deploy` now owns the deployment lock, an integrity-checked SQLite backup, exact version verification, and per-attempt recovery receipts for both manual and CI deployments. `ci-deploy` validates its wire boundary and delegates to the shared helper without nested locks or duplicate backups. Legacy `.env` settings fail closed without revealing input, and daily SQLite snapshots retain the 14-day policy only in a dedicated namespace.

Operators must reinstall all three helpers, install jq/sqlite3/util-linux/curl/gzip/coreutils and change the daily cron to `monochange-sqlite-backup --daily`. Move legacy optional `.env` settings into `app.env`, retaining only `MONOCHANGE_IMAGE` in `.env`. Manual calls require the full image SHA and stable version; use `--rollback` explicitly for older images. Use `--bootstrap` only for an empty new server; routine deployments now refuse a missing database. Recovery receipts distinguish desired/running/last-verified identity and preserve every release snapshot; no database is restored automatically.

The app CI workflow runs the installed-helper regression suite in an isolated Linux container. Fixed-path verification refuses host installation, and test/fixture changes trigger this check directly.

A daily retention failure reports an error while retaining the newly validated snapshot. Failed SQLite verification commands report the failing pragma without exposing database contents.

#### Explain GitHub release body limits and Rust settings migration

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #880](https://github.com/monochange/monochange/pull/880)

The GitHub automation guide now describes how oversized release notes retain complete entries and metadata, how continuation links fit within the size budget, and how to lower the release and PR limits. The 0.22 migration guide shows Rust callers how to initialize `ProviderReleaseSettings::max_body_chars`; existing TOML configurations need no changes.

- **Split the browser identity routes.** Login, connect, account, and OAuth callback now use separate lazy routes. The bundle-size check compares the callback chunk with the original callback baseline by route name, while the total budget continues to include all four chunks. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)

#### Declare backend ownership in each leptoskit application

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)

Both web configurations explicitly select the shared `monochange_app` server. Native and extension configurations explicitly disable backend discovery, so a host-packaging stub cannot be mistaken for an API server. Browser package selection remains in `build.client_package`.

```toml
# Website and projects web configurations
[build]
backend = true

# Native and extension configurations
[build]
backend = false
```

#### Enforce SDK initial-load budgets before publishing web assets

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)

Both web frontends now set the SDK's Brotli initial-load budget, so `build:web` and the deployment image stop before publication when either entry exceeds its limit. The measured initial loads leave approximately 5% headroom. The existing whole-site, per-file and raw-asset gates remain unchanged.

Website (`app/leptoskit.toml`):

```toml
[bundle]
budget_kib = 315
```

Projects (`app/projects/leptoskit.toml`):

```toml
[bundle]
budget_kib = 302
```

#### Move the leptoskit pin to the 2026-11-10 upstream revision

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #891](https://github.com/monochange/monochange/pull/891)

The web app, native workspace, browser extension, and the managed `leptoskit` CLI now build against one reviewed upstream commit, `fa3d29f8e8c837b19f92f1e9a95f699803543767` (upstream `main` captured on 2026-11-10), instead of the 2026-11-09 revision. Every direct dependency in the three workspaces and the root CLI pin name the same immutable full Git revision, and all three lockfiles resolve it, so `cargo generate-lockfile` retains the source. Before:

```toml
# Cargo.toml [workspace.metadata.bin]
leptoskit_cli = { version = "0.3.0", git = "https://github.com/leptoskit/leptoskit", rev = "a93214edc8e5f6c7999c628285093f15cf188639", locked = true, bins = ["leptoskit"] }
```

After:

```toml
# Cargo.toml [workspace.metadata.bin]
leptoskit_cli = { version = "0.3.0", git = "https://github.com/leptoskit/leptoskit", rev = "fa3d29f8e8c837b19f92f1e9a95f699803543767", locked = true, bins = ["leptoskit"] }
```

Reinstalling the lint CLI keeps the same provenance guard as before; only the revision moves:

```bash
cargo install --git https://github.com/leptoskit/leptoskit \
  --rev fa3d29f8e8c837b19f92f1e9a95f699803543767 --locked --force leptoskit_cli
```

The native workspace now pins `wasm-bindgen` to `=0.2.129` in `monochange_native_app`, matching the pins the web app and extension already carry. The pinned CLI embeds `wasm-bindgen-cli-support` 0.2.129, so a natively resolved 0.2.130 family would make the CLI reject the produced wasm. The native lock resolves the whole family at 0.2.129 with its paired `js-sys`/`web-sys` 0.3.106.

#### Resolve leptoskit from the upstream Git revision

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #884](https://github.com/monochange/monochange/pull/884)

The web app, native workspace, and browser extension now build against one reviewed upstream `leptoskit` commit instead of separate crates.io 0.3.0 copies. Every direct dependency in the three workspaces names the same immutable full Git revision, and every lockfile resolves that revision, so release preparation can run `cargo generate-lockfile` without moving a single `leptoskit*` source. Branch specs (`branch = "main"`) re-resolved to upstream's moving head during release lockfile refresh and drifted past the revision the managed CLI was verified against; the source guard then failed the release job. Upgrades are deliberate: change every manifest rev, the `[workspace.metadata.bin]` `leptoskit_cli` rev, and the three lockfiles together in one reviewed change.

`lint:leptoskit` now reads the same pinned revision from `[workspace.metadata.bin]`, verifies every `leptoskit*` lockfile source against it, and force-installs the CLI when the cached binary came from crates.io or another revision. A broad `.bin` cache restore can no longer leave the lint task running a stale same-version binary:

```bash
devenv shell lint:leptoskit
# manual equivalent:
cargo install --git https://github.com/leptoskit/leptoskit \
  --rev a93214edc8e5f6c7999c628285093f15cf188639 --locked --force leptoskit_cli
```

The web app additionally pins `wasm-bindgen` to `=0.2.129` (with its `js-sys`/`web-sys` family resolving to 0.3.106) to match the bindgen ABI the pinned CLI generates against. Floating `0.2` resolved 0.2.130, whose generated JS glue imports a `__wbindgen_externref_xform__` module specifier the served bundle does not provide, so every route failed hydration in the browser with `TypeError: Failed to resolve module specifier "__wbindgen_externref_xform__"` while the build and size budgets still passed. The extension already carried the same pin.

- **Compile navbar account-menu DOM work only for wasm.** The account menu's `close` method previously hid its browser-only DOM operations behind the repository's custom patch-coverage ignore markers. Host tests resolve the disclosure `NodeRef`s to `None` and return early, so those lines could never execute under native coverage, while Codecov did not honor the custom markers. The DOM work now lives behind the same `cfg(target_arch = "wasm32")` boundary already used by the route-change effect: native SSR and host tests keep the noop contract, browser behavior is unchanged, and the custom ignore markers are removed. The web navigation verification journey now also exercises the real desktop account disclosure (open, focus inside, Escape closes and restores focus), an Escape on an already-closed menu that does not steal focus, mobile Escape dismissal, and route-change menu/panel collapse. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Remove obsolete split modules from assembled web builds.** The web asset plugin now uses each entry's split manifest to remove obsolete WASM chunks and their compressed copies from that entry's namespace. Incremental builds retain chunks still named by the deployed manifest; an invalid manifest stops assembly before pruning. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Discover body fonts in the server-rendered head.** Both browser entries preload the existing Hanken 400 and 700 WOFF2 files with anonymous CORS. Browsers can request body and navigation fonts before the shared stylesheet finishes loading, shortening the HTML-to-stylesheet-to-font discovery chain. Deploy the updated server alongside the existing font assets at `/fonts/hanken-400.woff2` and `/fonts/hanken-700.woff2`. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Reduce shared stylesheet transfer overhead.** The web build drops unused selectors for the retired navigation and footer layouts, old homepage sections, and release-table icon wrappers from the shared stylesheet. Rebuild both browser entries to deploy the smaller CSS asset through the existing static-asset compression pipeline. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Track both web frontend budgets.** The website size gate now tracks each browser entry, its routes, the shared stylesheet, and deployed fonts against the approved dual-entry baseline. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Share JSON decoding for projects resources.** Projects route resources now decode their SSR JSON with the same byte-based decoder used by server functions. The JSON wire format remains unchanged, while duplicate WebAssembly decoders for the same response types can be eliminated. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Share the stylesheet between web entries.** The website and projects entries now load `/pkg/site/monochange_app.css`. Builds compile it from both frontends' source files and remove the obsolete projects stylesheet and its compressed copies from the assembled site. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Split the projects landing route.** The projects landing page and organization details now load separate browser chunks. This reduces each page's route transfer at the cost of additional shared code in the complete bundle. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Reduce shared project rendering code.** `ProjectCard` shares a type-erased render boundary across the workspace and organization routes while retaining normal hydration. Both callers consume its view directly. Decorative arrow SVGs use Leptos' inert markup representation. Header and workspace resource reads also borrow their results before selecting the fields needed for display. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Generate WOFF2 fonts during web builds.** The web asset plugin now requires `pyftsubset` with Brotli support. It generates deployable WOFF2 subsets from `app/public/fonts` and removes the copied TTFs from the assembled site. The devenv shell, CI, and Docker builder provide the required tool. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)
- **Keep navigation verification browser cleanup bounded.** The navigation verifier writes its owned Chrome diagnostics to a temporary file. Browser helpers that inherit stderr no longer retain a pipe in the verifier after Chrome exits. Process-exit checks, cleanup deadlines and failure reporting remain enforced. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)

#### Publish the web site only after both frontends build and keep manifests fresh

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)

The leptoskit tooling plugin used to move each frontend's build output into the shared site tree inside `after_build` — which the leptoskit CLI runs before its bundle report and `bundle.budget_kib` check. A build the SDK's own budget rejected could already be visible at `app/target/site`, the SDK's report measured zero bytes because its files had been moved, and the merge-based assembly kept stale public files and chunks from earlier builds alive forever.

`build:web` now gives each build a fresh per-invocation scratch directory under the ignored `.monochange/local/web-build/<run>/`, so the SDK's report and budget always measure the real, complete scratch bundle. After BOTH builds succeed, `scripts/leptoskit-plugin/publish-web-site.ts` stages one deployment tree from those scratch outputs alone (both `pkg/` namespaces, the public assets, the WOFF2 fonts), verifies the two builds compiled byte-identical shared CSS, verifies every split manifest references only chunks that exist (and vice versa), precompresses the staged tree with upstream's gzip/brotli set, validates completeness, and promotes it over `app/target/site`. Any failure — build, validation, compression, or the promotion swap itself — leaves the last good published site byte-identical. The promotion is a two-rename directory swap; the window between the renames is documented in the publisher header (readers that open the site root in that instant see a missing directory), and the image copy only reads the tree after `build:web` exits.

One publication at a time owns a destination through an exclusive lease, `.<site>.publish-lease` beside the site root (`app/target/.site.publish-lease`). The lease appears with its owner record already in place. It holds the staging tree and the superseded tree, so a concurrent publication of the same site fails before it changes anything, and sibling destinations stay independent. The publisher never removes anything beside the site root by name pattern. If the swap and its rollback both fail, the last-good tree stays in `<lease>/previous` under the still-held lease, and the error names it. A post-commit failure to remove the superseded tree is a warning with a retained `.<site>.publish-retired-<token>` directory, not a failed publication, and later publications never touch it.

The publisher never guesses that an interrupted owner is dead. Every later publication fails closed, naming the owner, the retained trees, and the recovery command. After confirming that owner is not running anywhere, run that command. Recovery restores the last-good tree when the site root is absent. Otherwise it sets the retained tree aside as `.<site>.publish-retained-<token>` and never deletes it.

`app/crates/monochange_app/src/frontend.rs` reads split manifests fresh on every non-deferred render instead of caching them in a process-global `Mutex`: a manifest that was missing, invalid, or from the previous generation now reaches the very next request after publication or repair, including same-sized edits no size-keyed cache could observe.

`app/deploy/verify-image.sh` requests the actual dual-namespace image layout — `/pkg/site/*` and `/pkg/projects/*`, the shared stylesheet, both manifests with every chunk they reference, the fonts and shell-referenced public assets, and both gzip and brotli precompressed responses — instead of the retired flat `/pkg/monochange_app.*` URLs.

Docker, CI, and the devenv task use the same build-and-publish sequence, and their assembled-site checks assert the real layout (no duplicate stylesheet in `pkg/projects`, fonts and precompressed twins present).

##### Example

```bash
# Build both frontends into fresh scratch outputs, then publish one validated tree
devenv shell build:web --release
# Compare the published tree against the unchanged size baseline
pnpm node scripts/website-size.ts check --site-root app/target/site
# Only after an interrupted publication, once its named owner is confirmed gone
node scripts/leptoskit-plugin/publish-web-site.ts --site-root app/target/site --recover-abandoned-lease <token>
```

#### Keep operator settings across website deployments and explain failed rollouts

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #892](https://github.com/monochange/monochange/pull/892)

`/usr/local/bin/monochange-deploy` rewrites `/opt/monochange/.env` with the image tag on every rollout, so any setting stored there, such as `MONOCHANGE_FEEDBACK_MODEL`, disappeared at the next deployment. The Compose file now loads optional, non-secret runtime settings from `/opt/monochange/app.env`, which no deployment touches:

```yaml
services:
  app:
    env_file:
      - path: /opt/monochange/app.env
        required: false
```

`MONOCHANGE_FEEDBACK_MODEL` and `MONOCHANGE_FEEDBACK_COMPLETIONS_URL` are no longer interpolated from `.env`; set them in `app.env` instead. The app's defaults are unchanged.

The deploy helper moves out of `harden-droplet.sh` into `app/deploy/digitalocean/monochange-deploy.sh`, so the running server can be updated without rerunning the hardening. It now waits up to 120 seconds for `/health` instead of giving up after 20 attempts one second apart, and when the app never becomes healthy it prints `docker compose ps app` and the last 60 lines of the app's startup log, so the Actions log names the cause (for example a missing `TOKEN_ENCRYPTION_KEY`). `harden-droplet.sh` also installs `ci-deploy.sh` as `/usr/local/bin/monochange-ci-deploy` and refuses to run unless both helpers sit beside it.

Operators apply this once on the existing Droplet before the release that carries it deploys:

```bash
scp -F app/deploy/digitalocean/ssh_config app/deploy/digitalocean/docker-compose.yml monochange-production:/opt/monochange/
scp app/deploy/digitalocean/monochange-deploy.sh app/deploy/digitalocean/ci-deploy.sh root@167.172.51.148:/root/
ssh root@167.172.51.148 'install -o root -g root -m 0755 /root/monochange-deploy.sh /usr/local/bin/monochange-deploy \
  && install -o root -g root -m 0755 /root/ci-deploy.sh /usr/local/bin/monochange-ci-deploy \
  && rm /root/monochange-deploy.sh /root/ci-deploy.sh'
```

`app/DEPLOY.md` is reorganised around the release path: a release-day checklist, per-release operator notes (the required `TOKEN_ENCRYPTION_KEY` field from 0.4.0 and the GitHub App **Actions: Read and write** permission that hosted workflow dispatch needs), the automated `app.yml` deployment step by step, recovery and rollback for a failed rollout, and every optional runtime setting. The shared release-notes guide now names `app.yml`, not `ci.yml`, as the workflow that deploys the website.

#### Serve the wasm-bindgen externref helper before hydration

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #883](https://github.com/monochange/monochange/pull/883)

cargo-leptos 0.3.6 hides the `reference-types` feature so wasm-bindgen will not rewrite split modules. wasm-bindgen 0.2.130 still emits this import in `monochange_app.js`:

```js
import * as __wbindgen_externref_xform__ from "__wbindgen_externref_xform__";
```

Browsers reject that bare specifier, so `hydrate()` never runs. The document shell now declares an import map before the hydration module:

```html
<script type="importmap">
	{"imports":{"__wbindgen_externref_xform__":"/wbindgen-externref-xform.js"}}
</script>
```

`app/public/wbindgen-externref-xform.js` exports `__wbindgen_externref_statics_end` as `1028`, the same boundary the generated glue uses when it leaves the four reserved externref slots (indices 1024 through 1027) alone. A site build must keep serving that file from the site root.

#### Minify the wasm-split loader in release builds

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #886](https://github.com/monochange/monochange/pull/886)

The website release build now minifies `pkg/__wasm_split.*.js` — the loader module that maps every lazy route to its WASM chunks — before the site is packaged. cargo-leptos `js-minify` covers the main `monochange_app.js` glue only, so the loader kept its per-chunk route comments and full identifiers and failed the committed bundle-size gate (2,234 gzip bytes versus the 1,789-byte baseline, above the 5% per-file allowance). After minification the loader ships at 1,484 gzip bytes (6.9 KB raw, down from 26.3 KB), within every gate with no baseline, tolerance, or exclusion change.

The step is part of the release pipeline, not a manual chore: app CI runs it between `cargo leptos build --release --split` and the size/Lighthouse checks, and the Docker release builder runs it before the site is copied out and precompressed, so the deployed gzip/brotli siblings always match the gate-measured bytes. The upstream `leptoskit build --release` web build emits the same loader shape; pass its output directory to the script's `--site-root` when building that way:

```bash
cargo leptos --manifest-path app/crates/monochange_app/Cargo.toml build --release --split
pnpm node scripts/minify-split-loader.ts --site-root app/target/site
```

The workspace permits the install script for `esbuild@0.28.2` through pnpm's `allowBuilds` setting so strict CI installs can provision its native binary.

The rewrite is guarded: `scripts/minify-split-loader.ts` replaces a loader only when the minified output still exports the same `__wasm_split_load___<route>` initializers and still references the same `/pkg/monochange_app.js` import and `./chunk_N.wasm` / `./split___<route>.wasm` URLs. A minifier regression or an unexpected loader shape fails the build instead of reaching a deployment. esbuild is resolved from `MONOCHANGE_ESBUILD_BIN`, the workspace `node_modules/.bin/esbuild` devDependency, or `PATH`, in that order; the Docker builder installs a pinned esbuild binary (registry integrity digest verified) plus Node 24 so the identical script runs there.

<details>
<summary><strong>🔨 Refactor</strong></summary>

#### Pin the navigation route-change collapse contract

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)

The navbar's route-change effect now shares one `close_menus` collapse core with the Escape handler, and the effect is created only in the browser build, like `color_mode`'s: reactive effects compile out of the server build, so the served markup and client behavior are unchanged while the collapse contract itself is unit-testable on the server. The web navigation verification journey now also opens the mobile panel on the homepage, follows a section link from that panel, and asserts the client-side route change collapses it (`aria-expanded` back to `false` and the panel removed from the accessibility tree).

The repository's JavaScript lint gate is warning-free again without weakening any rule: the deferred hydration loader keeps its bare function-expression contract (documented where the lint exception is declared), and the sequential polling, shutdown, and per-route benchmark loops keep per-site exceptions naming the ordering dependency each one protects.

</details>

<details>
<summary><strong>🧪 Testing</strong></summary>

#### Rehearse the SQLite release upgrade and the deployed backup mechanism

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #893](https://github.com/monochange/monochange/pull/893)

Release readiness for the website now rests on executed proofs instead of plans. A new rehearsal suite in `monochange_app_db` builds an authentic v0.3.3 database (the real first-five migration implementation, byte-identical to the `monochange_app/v0.3.3` tag, seeded from checked-in fixtures in `fixtures/tests/app-sqlite-release-upgrade/`), runs the current migration runner over it, and asserts every legacy row — including the opaque token bytes — survives unchanged, `_welds_migrations` records exactly the two candidate migrations (`006_create_oauth_state_uses`, `007_create_client_sessions`) while keeping the original five rows' ids and timestamps, `quick_check` and `foreign_key_check` stay clean, `users.session_epoch` defaults existing users to 0, and a second migration pass is idempotent without touching post-upgrade writes. A further test pins the rollback limit honestly: the v0.3.3 runner's migration list no-ops against the upgraded schema, which establishes reader/runner compatibility — not old-binary compatibility.

The deployed backup mechanism gets the same treatment: `scripts/sqlite-backup-rehearsal.ts` rehearses `harden-droplet.sh`'s `monochange-sqlite-backup` (`sqlite3 .backup` with a 14-day retention sweep) against the same schema fixtures. It proves a snapshot taken while committed rows sit in an uncheckpointed WAL contains them (a torn copy of just the database file misses those rows), that the snapshot restores into a different directory with clean `quick_check`, `integrity_check`, and `foreign_key_check`, that the source database is untouched, and that corrupt or missing snapshots are refused before any last-good destination is replaced.

`app/DEPLOY.md` and `app/deploy/digitalocean/OPERATIONS.md` now carry the concrete contract these proofs back: a data-preserving rollback checklist (back up before migrations, prefer image rollback, restore only deliberately after verifying the snapshot, preserve the Droplet's engine, bind mounts, secrets, and Caddy state, retain previous images and receipts), the honest limits (off-Droplet backups still unset, restores discard writes newer than the snapshot), and the rehearsal's evidence trail.

The rehearsal also confirms its owned SQLite writer has actually stopped before cleanup returns. A real stubborn-process regression covers forced termination and repeated cleanup; startup and operation failures share the same bounded teardown.

</details>

## 0.4.0

### 💥 Breaking Change

#### Upgrade the website to Leptos 0.9 with leptoskit_rpc

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #870](https://github.com/monochange/monochange/pull/870)

The nested `app/` workspace now links **Leptos 0.9.0-rc1** and **leptoskit / leptoskit_rpc 0.3**. Product RPCs register through `#[api]` on stable `/api/<endpoint>` paths, while cookie auth and Class B Axum mounts stay on hand-rolled routers.

Wire DTOs that used to live only under `monochange_app::server_fns::*` now define in `monochange_shared` and are re-exported from the old modules. Prefer the shared crate for new guests (extension / native); existing `server_fns` imports keep compiling through the re-exports, but public-api tooling treats the move as a major surface change.

```rust
// Prefer shared DTOs for new callers:
// Existing app code can keep:
use monochange_app::server_fns::auth::SessionUser;
use monochange_shared::OrganizationSummary;
use monochange_shared::SessionUser;
use monochange_shared::WebsiteRelease;
```

Form actions and server-fn clients must post to the stable endpoint path (no build-hash suffix):

```text
# before (Leptos 0.8 hashed server-fn URL)
POST /api/get_session/<build-hash>

# after (leptoskit_rpc #[api] + endpoint = "…")
POST /api/get_session
```

- **Leptos 0.9 stack** — Workspace `leptos`, `leptos_axum`, `leptos_meta`, `leptos_router`, and `server_fn` move to `0.9.0-rc1`. App `ssr` enables `leptoskit/server` + `leptoskit/rpc-ssr`; `hydrate` enables `leptoskit_rpc/web`.
- **`#[api]` on product RPCs** — All 31 product `#[server]` functions take `#[api(crate = "leptoskit_rpc", tag = "…")]` with `endpoint = "<name>"`.
- **Server / hydrate wiring** — `main` uses `leptoskit::server` re-exports; hydrate installs `any_spawner::Executor::init_wasm_bindgen` before `leptoskit_rpc::client::configure` (the live client spawns immediately), then `hydrate_lazy`. Class B routes still mount via `monochange_backend::machine_api_router`.
- **0.9 adaptation** — Rate-limit server-fn errors use `ServerFnError::ser()` response parts; OAuth fallback alias tests send `Host` so SSR path normalization matches `rate_limit::surface`.

### 🚀 Feature

#### Authenticate embedded feedback and approve connected-agent plans

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #860](https://github.com/monochange/monochange/pull/860)

Adds the project-scoped `/api/v1/feedback/{project}` API for backend-issued sessions, private draft conversations, suggestions, idempotent submissions, discussion, votes, worker pairing, leased jobs, and authenticated results. Register integrations in the feedback console, then exchange a backend credential at `POST /sessions` with the host-authenticated `user_id` and `surface` (`web` or `mobile`). Backend credentials are returned once; only digests persist. Rotating an integration expires its existing sessions.

Projects default to manual planning and can enable automatic planning for new eligible feature requests with a daily budget. Plans bind a material brief revision and pinned repository commit; only a maintainer can approve implementation. Connected workers receive scoped jobs and return changes for draft PRs. Existing direct GitHub build dispatch cannot bypass approval for projects using the new flow.

Set `OPENROUTER_API_KEY` and `MONOCHANGE_FEEDBACK_MODEL` to enable semantic suggestions and assistant replies. `MONOCHANGE_FEEDBACK_COMPLETIONS_URL` can select a deployment-owned compatible endpoint. Without a configured provider, or when it fails, keyword suggestions remain available and drafts are preserved. Existing project feedback documents load with empty integration and planning state; intake, quotas, idempotency, and jobs share the existing transactional save.

Fallback bug triage records suggested verification steps as unverified. Providing screen context alone no longer marks a bug as reproduced.

For hosts that only accept HTML, embed `/embed/feedback.html?organization=acme&project=web` with `allow-scripts allow-same-origin allow-popups allow-popups-to-escape-sandbox`. The hosted popup exchanges first-party monochange sign-in at `POST /api/feedback/iframe-session`, validates its Origin against `MONOCHANGE_GITHUB_CALLBACK_URL`, and issues a ten-minute feedback-only token. It shares the public portal identity and quotas, keeps tokens out of the parent and URLs, and requires no host secret. Deploy the generated frame HTML, CSS, and JavaScript alongside the API. Configure the host CSP `frame-src` for the deployment; preserve popup opener access and permit framing on the feedback page.

#### Start the leptoskit SSR layout with shared DTOs and Class B mounts

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #870](https://github.com/monochange/monochange/pull/870)

The website workspace gains the first leptoskit-shaped scaffolding without changing cookie auth or Class B machine HTTP contracts.

- **`app/leptoskit.toml`** — SSR on port 3000, `client_package = "monochange_app"`, route-split WASM bundles. Prefer `leptoskit serve` / `build` when the CLI is installed; `cargo leptos` remains the documented fallback.
- **`monochange_shared`** — Wire DTOs (`SessionUser`, repo/org/project summaries, installation status, book and website release types) for the WASM UI and future extension/native guests. Existing `server_fns` modules re-export them so imports stay stable.
- **`monochange_backend`** — `machine_api_router` mounts GitHub webhooks, hosted release OIDC, embed SDK, and CLI task routes via `monochange_app_api::api_router`.
- **Stable RPC paths** — Product `#[server]` functions set `endpoint = "<name>"` so leptoskit_rpc `#[api]` can pin routes after the Leptos 0.9 bump. Workspace deps declare `leptoskit` / `leptoskit_rpc` 0.3; the app still compiles on Leptos 0.8 until that upgrade lands.

#### Add leptoskit Android and iOS app scaffolds

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #868](https://github.com/monochange/monochange/pull/868)

`app/native/` now holds a leptoskit workspace for the monochange Android and iOS clients. Native screens share `monochange_rpc` types with the future web port and call `leptoskit_rpc` for the auth handshake (`SessionProbe` / `auth_handshake`) and project list (`ListProjects`). UI chrome follows the Release floor `--mc-*` tokens (mist page, ink, teal accent) instead of the legacy violet marketing palette.

Build the hosts with the leptoskit CLI after installing platform SDKs:

```bash
cd app/native
leptoskit doctor
leptoskit native prepare android   # needs ANDROID_HOME, SDK 36, NDK
leptoskit build --platform android
leptoskit native build ios-simulator   # macOS + Xcode only
```

Generated `.leptoskit/` trees stay local. Release signing still needs `LEPTOSKIT_ANDROID_*` keystore secrets on Android and an Apple development team on iOS; this scaffold does not ship store credentials.

### 🐛 Fixed

- **Reboot the production Droplet after security updates.** The DigitalOcean hardening script now writes `/etc/apt/apt.conf.d/51unattended-upgrades-auto-reboot`, so the Droplet reboots at 07:30 UTC when an unattended Ubuntu security upgrade leaves `/var/run/reboot-required`. The new kernel is activated instead of waiting for a manual reboot; Compose restarts the app and Caddy because both use `restart: unless-stopped`. The operations guide documents the window, the short outage, and the Docker repository updates that still need a manual pass. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #856](https://github.com/monochange/monochange/pull/856)

#### Integrate feedback with copyable installation examples

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #864](https://github.com/monochange/monochange/pull/864)

The installation guide now distinguishes a paste-only iframe using monochange sign-in from SDK and script integrations using a host-authenticated backend. It includes package installation commands, a complete backend token-handler example, a plain HTML script example, Flutter token conversion, and an end-to-end verification checklist.

Coding agents can follow `docs/agents/feedback-widget.md` from `AGENTS.md` to locate the implementation and run focused local checks. The guide distinguishes project slugs from numeric IDs and makes publication and deployment prerequisites explicit. Runtime behavior is unchanged; the patch note covers the rendered website documentation.

#### Add canonical `monochange format` command

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #854](https://github.com/monochange/monochange/pull/854)

Adds `monochange format` to rewrite `monochange.toml` in the idiomatic layout: top-level sections in a fixed order, `[package.*]` and `[group.*]` tables sorted by id, group members sorted alphabetically, and `[cli.*]` workflows last. Comments stay attached to their tables and the document header stays at the top.

The command formats the result with taplo using the repository's tab-based, 100-column configuration, so it does not require a separate taplo installation. Use `monochange format --check` to fail CI when the file needs formatting without writing changes. The repository's `lint:monochange` task and CI lint job now run that check.

#### Encrypt GitHub OAuth tokens at rest and scope the hosted API token

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #834](https://github.com/monochange/monochange/pull/834)

Two changes to the monochange.dev database and hosted API:

- GitHub user access and refresh tokens are now sealed with AES-256-GCM before they are written to the SQLite database, under a new required `TOKEN_ENCRYPTION_KEY` secret (`auth` section for production, generated in development and CI). A database file disclosure no longer leaks live GitHub tokens. Rows written before this change stay readable in plaintext and are replaced with sealed values the next time the token refreshes; rotating the key invalidates stored tokens and users simply sign in again.
- The deployment-wide `MONOCHANGE_TOKEN` no longer authorizes every connected repository. It only acts on repositories listed in the new `MONOCHANGE_TOKEN_REPOSITORIES` environment variable (`owner/repo`, comma or whitespace separated, case-insensitive). Deployments that rely on the token must list the repositories it may act on, or use GitHub Actions OIDC tokens, which were already restricted per repository.

#### Tighten hosted endpoint verification and startup secret checks

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #833](https://github.com/monochange/monochange/pull/833)

The hosted release API now rejects three edge cases that previously slipped through verification:

- GitHub Actions OIDC tokens must be signed with RS256. The verifier pinned the algorithm to the token header before, so a token that named any other algorithm selected how its own signature was checked.
- The server refuses to start when the resolved `JWT_SECRET` is an empty string. An empty secret is a valid HMAC key, and a profile that resolves the secret to nothing would have signed session tokens anyone could forge.
- The unreachable "repository mismatch" check after the connected-repository lookup is gone. Repositories are resolved by exact full name, so the check could never fire.

The constant-time comparison used for `MONOCHANGE_TOKEN` checks is now shared with the webhook signature path instead of existing as two identical copies.

#### Create hosted release trees with inline file contents

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #861](https://github.com/monochange/monochange/pull/861)

When a workflow sends release file contents instead of a prebuilt tree, `POST /api/release-commits` now creates the tree in one GitHub request with the contents inline. Previously the app made one installation-token `POST /git/blobs` request per file. Releases touching more than about 80 files hit GitHub's secondary rate limit for content creation, and the request outlasted the CLI's timeout. That broke the fallback the CLI uses when its own token cannot build the tree.

Deletions are still sent as `"sha": null`. A release with more than 4 MiB of file contents is split into tree requests chained through `base_tree`. A request that lists no files still creates a tree on the base commit, as before.

#### Protect hosted release branches and pull requests

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #852](https://github.com/monochange/monochange/pull/852)

Enforce hosted release authorization before creating or moving any branch. Every hosted commit must name a `<prefix>/release` branch distinct from both the requested base and the repository default. Older clients may still create or fast-forward a valid release branch without naming a base; replacing an existing release branch requires a named base whose head still matches the prepared commit.

Hosted pull request updates now require a server-signed marker bound to the PR number, node ID, bot creator, repository IDs, head, and base. Requests cannot adopt an unrelated PR or enable auto-merge on it. Existing release PRs without a marker must be closed and recreated once; rotating the deployment JWT signing secret also requires recreating open release PRs. If GitHub creates a release PR but its marker update fails, close the newly created PR identified in the error before retrying.

- **Preserve workflows in hosted release commits.** Hosted release commits now compare the final `.github/workflows` subtree with the prepared base for both uploaded files and prebuilt trees. Requests fail if workflow contents change, a protected directory becomes a file, or a tree listing is incomplete. Uploaded file paths must be canonical repository-relative paths and cannot replace or delete `.github` or `.github/workflows`; uploads to other `.github` files remain supported. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #849](https://github.com/monochange/monochange/pull/849)
- **Enforce leptoskit component lint rules in CI.** Website UI crates now run the official `leptoskit lint` rules (`component_attribute`, `markup_style` with `markup = "view"`) through `lint:leptoskit`, which is wired into `lint:all`, `lint:push`, and the CI lint job. Maintainers install `leptoskit_cli` via `cargo bin --install` (pinned in `[workspace.metadata.bin]`) and run `devenv shell lint:leptoskit` locally; each `leptoskit.toml` should deny those rules so `#[component]`, `view!` markup, and PascalCase component names stay required. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #871](https://github.com/monochange/monochange/pull/871)

#### Bound OAuth work and reject callback replays

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #847](https://github.com/monochange/monochange/pull/847)

`/login` and `/auth/callback` now consume the anonymous address bucket before server rendering. Valid signed callback state is atomically consumed in SQLite before the provider exchange, so replaying the original cookie cannot trigger more exchanges, including after a process restart. Failed exchanges also consume their state; start a fresh sign-in attempt to retry.

The application runs migration `006_create_oauth_state_uses` on startup. Consumed states are retained until their signed expiry and pruned during subsequent callbacks. Provider token requests have a ten-second timeout, and each app process permits at most sixteen concurrent exchanges. Saturated requests can retry their existing state because capacity rejection happens before consumption. Existing rate-limit environment variables continue to control the address bucket.

#### Bound OIDC signing key refreshes independently of token key IDs

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #848](https://github.com/monochange/monochange/pull/848)

Authentication for `POST /api/release-commits` and `POST /api/release-requests` now rejects unknown signing key IDs from a fresh JWKS cache without contacting GitHub. The cache remains valid for ten minutes. Concurrent refreshes share one request, and a shared retry deadline of thirty seconds prevents failed or cancelled attempts from restarting once per key ID. Invalid tokens can no longer force one provider request per attempt.

A signing key first published by GitHub while the cache is fresh is not fetched or accepted until that ten-minute cache expires.

JWKS responses must have a successful HTTP status and fit within 64 KiB before parsing. Failed refreshes retain the previous key set but do not authorize tokens with expired keys. Deployments and workflow callers need no configuration changes.

#### Authorize organization access through stable provider identities

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #846](https://github.com/monochange/monochange/pull/846)

Bind dashboard organization and project authorization to each installation's stable provider account ID. GitHub membership responses must identify that same account before granting repository access, and legacy installations resolve their identity before membership checks. Organization logins remain display metadata; duplicate stored logins cannot select a tenant through public project routes. No configuration or data migration is required.

#### Patch pulldown-cmark exponential parsing advisory

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #865](https://github.com/monochange/monochange/pull/865)

Bump `pulldown-cmark` from 0.13.4 to 0.13.6 in `app/Cargo.lock`.

0.13.4 grows its event stream exponentially when `ENABLE_WIKILINKS` is set, so a crafted markdown document pins a CPU for the duration of the parse (RUSTSEC-2026-0414, RUSTSEC-2026-0433). Patch-level move inside the existing 0.13 requirement; no source changes.

- **Clarify release-record validation in the CLI guide.** The `CommitRelease` reference now keeps mismatch protection enabled in its formatting example. It explains which prepared fields are checked, how rejected records preserve neighboring release records, and when deliberate recovery with `update_release_json = true` is appropriate. Whitespace and JSON key ordering require no configuration change. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #850](https://github.com/monochange/monochange/pull/850)

<details>
<summary><strong>🔒 Security</strong></summary>

#### Mitigate CVE-2026-98432 on the production Droplet

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #859](https://github.com/monochange/monochange/pull/859)

The DigitalOcean deployment now installs a persistent nftables prerouting rule that drops inbound IPv4 TCP packets carrying IP options. The vulnerable kernel path is only reachable from a connection request with IP options, and the prerouting hook covers both host services and Docker-published Caddy ports.

Ubuntu 24.04 has not released a fixed kernel yet. The rule stays until the Droplet runs a kernel that includes the upstream fix, then `mitigate-cve-2026-98432.sh --remove` removes the table, unit, and helper files. Caddy also runs with `GODEBUG=multipathtcp=0` so its Go listener does not negotiate MPTCP, which the demonstrated exploit requires; the underlying bug is not MPTCP-specific, so the kernel update remains the real fix.

</details>

## 0.3.3

### 🐛 Fixed

#### Read GitHub OAuth token replies without discarding the error

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #828](https://github.com/monochange/monochange/pull/828)

`exchange_code` and the user-token refresh in `github_user_access_token` no longer call `reqwest::Response::json` on `POST /login/oauth/access_token`. They read the body as text and parse it in `parse_token_response`, which:

- returns `GitHub rejected the sign-in: <error_description> (<error>)` when GitHub sends an OAuth error instead of a token;
- accepts `application/x-www-form-urlencoded` replies and lifetimes sent as strings (`"expires_in": "28800"`);
- reports any other body as `GitHub returned an unreadable token response (HTTP <status>, <content type>): <title or first 200 characters>` and logs it with `tracing::warn!`.

Refresh failures still ask the user to sign in again, but the server log now carries GitHub's reason.

- **Document native package release setup.** The hosted ecosystem guide now explains how to configure Swift, CocoaPods, and Gradle packages, including versioned files and registry credentials. Follow the native ecosystem sections when adding Apple or Android packages to an existing workspace; Gradle and CocoaPods publishing require `trusted_publishing = false` and their registry credentials. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #830](https://github.com/monochange/monochange/pull/830)

## 0.3.2

### 🚀 Feature

- **Commit release trees built by the workflow.** `POST /api/release-commits` accepts a `tree` instead of `files`. The server checks that the request names exactly one of them and that `tree` is a git object id, then creates the release commit on that tree without uploading any blobs. A tree whose `.github/workflows` differs from the base commit's is refused with `422 Unprocessable Entity` before anything is written, keeping the rule that the bot never changes GitHub workflows. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #812](https://github.com/monochange/monochange/pull/812)

#### Report installations that haven't approved the app's new permissions

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #819](https://github.com/monochange/monochange/pull/819)

GitHub keeps each GitHub App installation on the permissions it approved until an owner of the account accepts the app's newer request. The new `permission_updates` server function compares the app registration's permissions (`GET /app`) with each of the signed-in user's installations (`GET /app/installations/{id}`) and returns the accounts that are behind, the missing permissions, and the GitHub settings page where an owner reviews the request. The dashboard shows them in a notice above the repository connection panel.

`monochange_app_api::github_app` gains `GitHubAppAuth::requested_permissions`, `GitHubAppAuth::installation_permissions`, and `pending_permissions`, which lists permissions the app requests above the installation's level:

```rust
let requested = app.requested_permissions(&http).await?;
let granted = app.installation_permissions(&http, installation_id).await?;
let missing = monochange_app_api::github_app::pending_permissions(&requested, &granted);
```

A failed GitHub lookup leaves that account out of the list instead of failing the dashboard. No configuration changes are needed.

### 🐛 Fixed

- **Send a User-Agent when the app calls the GitHub API.** Hosted release commits failed with `502` and "failed to mint installation token" because `POST /app/installations/{id}/access_tokens` went out without a `User-Agent` header, which GitHub rejects with `403 Forbidden`. The shared `reqwest::Client` built in `AppState::new` now sets `User-Agent: monochange`, so installation token, tree, commit, ref and pull request calls all identify the app. No configuration change is needed; redeploy monochange.dev to pick it up. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #818](https://github.com/monochange/monochange/pull/818)

#### Document how command output is ordered

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #813](https://github.com/monochange/monochange/pull/813)

The progress output reference now explains the order every multi-line command follows: the plan, its details, the log, the detailed results, and a closing summary line. It includes a placeholder publish example and lists the new `publish_plan` JSON progress event.

The `PublishPackages` and `PlaceholderPublish` step references now say that results end with their summary line, and describe how a registry rate limit is reported and resumed.

#### Check hosted release credentials before reading the request body

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #810](https://github.com/monochange/monochange/pull/810)

`POST /api/release-commits` and `POST /api/release-requests` now verify the `Authorization` bearer token (GitHub Actions OIDC or `MONOCHANGE_TOKEN`) before reading the request body. A caller without valid credentials gets `401` without the server buffering its payload. Previously the JSON body, up to 32 MiB for release commits, was read and parsed before authentication.

The release commit body limit drops from 32 MiB to 8 MiB, which still fits monochange's own grouped release (about 3 MiB). A body over the limit now returns `413 Payload Too Large` instead of `400`, and the release request endpoint keeps a 2 MiB limit. An OIDC token for one repository still gets `403` when the request targets another, now before the repository lookup.

`AppState` gains `oidc: Arc<OidcVerifier>`, so the GitHub Actions JWKS is cached across requests instead of being fetched on every call.

- **Recommend `monochange publish` commands in the docs.** The guides, CLI step reference, and readme now use `monochange publish packages`, `monochange publish readiness`, and `monochange publish placeholder` in examples and instructions, and say that the matching `monochange step` commands remain supported for compatibility. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #811](https://github.com/monochange/monochange/pull/811)

## 0.3.1

### 🚀 Feature

#### Rate limit the API per client

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #807](https://github.com/monochange/monochange/pull/807)

The new `monochange_app::rate_limit` module limits Leptos server functions and the REST endpoints under `/api` with [`governor`](https://docs.rs/governor) GCRA buckets. Requests with a valid session cookie count against the signed-in user's bucket (300 a minute, burst 100); every other request counts against its client address (60 a minute, burst 30), with IPv6 addresses grouped by their /64. Pages, `/pkg` assets, `/health`, and `/api/github/webhooks` are not limited.

`X-Forwarded-For` is read from the right and only when the peer is a loopback or private address, which is Caddy in the production deployment. The server now runs with `into_make_service_with_connect_info::<SocketAddr>()` so the middleware sees the peer address.

Limited responses carry `RateLimit-Policy` and `RateLimit` fields, and a `429 Too Many Requests` adds `Retry-After`. Server function rejections use the `ServerFnError` encoding (`ServerError|Too many requests. Try again in 12 seconds.`); REST rejections use the endpoints' `{"message": ...}` JSON body.

Override the limits with environment variables; a value that isn't a whole number above zero stops startup:

```bash
MONOCHANGE_RATE_LIMIT_ANONYMOUS_PER_MINUTE=60
MONOCHANGE_RATE_LIMIT_ANONYMOUS_BURST=30
MONOCHANGE_RATE_LIMIT_AUTHENTICATED_PER_MINUTE=300
MONOCHANGE_RATE_LIMIT_AUTHENTICATED_BURST=100
```

#### Filter and paginate repository lists

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #804](https://github.com/monochange/monochange/pull/804)

The new `monochange_app::repository_filter` module holds the dashboard's list rules: `RepositoryFilter` reads the `q` (name search) and `org` (account) query keys and builds links with `href`, `matches_search` matches a repository's name (or `owner/name` when the search contains `/`), and `repository_page` returns one `REPOSITORIES_PER_PAGE` (20) page with its position and page count.

The new `components::repository_list` module renders them: `RepositoryFilters` (a GET search form that updates the URL in place after hydration), `RepositoryPager` (one list's pages), and `RepositoryPicker` (project-form checkboxes with a name filter that only hides rows, so hidden selections still submit). `RepoInfo` now derives `PartialEq` and `Eq`.

```text
/dashboard?q=web&org=acme
```

### 🐛 Fixed

#### Accept hosted release commits larger than 2 MiB

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #803](https://github.com/monochange/monochange/pull/803)

`POST /api/release-commits` now accepts request bodies up to 32 MiB (`monochange_app_api::RELEASE_COMMIT_BODY_LIMIT`). Before this change, the route used axum's 2 MiB default, so a grouped release that rewrote many changelogs was rejected with `400 Bad Request` (`length limit exceeded`) before it was authenticated. monochange's own release commits already send about 3 MiB of changelog content.

Other API routes keep the 2 MiB default. No configuration change is needed; deploy the new app version before switching a large repository's release workflow to `commit_backend = "hosted"`.

- **Document pnpm trusted publishing.** The trusted-publishing and configuration guides now explain that pnpm workspaces publish with `pnpm publish`, including under trusted publishing, that this requires pnpm 11.1.3 or later on the publishing runner, and why monochange passes `--no-git-checks` to pnpm. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #809](https://github.com/monochange/monochange/pull/809)

## 0.3.0

### 💥 Breaking Change

#### Require GitHub sign-in to write on the feedback portal

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #798](https://github.com/monochange/monochange/pull/798)

**Breaking change:** `server_fns::auth::get_login_url` now takes `next: Option<String>`, and `server_fns::portal::VIEWER_COOKIE` is removed along with the `__Host-monochange_viewer` cookie. Callers pass `None` to keep the old behavior:

```rust
// before
let url = get_login_url().await?;

// after
let url = get_login_url(None).await?;
```

`share_feedback`, `portal_vote`, and `portal_reply` now refuse a visitor without a valid session ("Sign in with GitHub to share.", "… to vote.", "… to join the discussion."). `portal_view` stays public: a missing, expired, or forged session reads as signed out, and `PortalSnapshot` gains `viewer: Option<PortalViewer>` with the visitor's login and avatar.

A signed-in visitor's per-project id comes from the new `monochange_app_api::feedback::account_viewer_id(secret, provider, external_id, project_id)`, an HMAC of `"{provider}:{external_id}"`. One account votes once per project, ids can't be linked across projects, and stored feedback never names a GitHub login. Items shared under a cookie id keep their votes and stay listed, but their author no longer sees them under "Yours".

Sign-in can return to where it started. `get_login_url(next: Option<String>)` and `oauth::login_state_returning(secret, next)` sign the destination into the OAuth state cookie, `VerifiedOAuth::return_to` carries it back, and `exchange_code` redirects there instead of `/dashboard`. `oauth::return_path` accepts only same-site paths, so absolute, protocol-relative, and backslashed values, control characters, and paths over 512 bytes fall back to the dashboard:

```text
/login?next=%2Fp%2Facme%2Fpocketbook%23roadmap  → back to /p/acme/pocketbook#roadmap
/login?next=%2F%2Fevil.example                  → /dashboard
```

#### Remove the placeholder roadmap and feedback server functions

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #786](https://github.com/monochange/monochange/pull/786)

**Breaking change:** the placeholder server functions that predate project feedback are removed from `monochange_app`, along with their HTTP endpoints:

- `server_fns::roadmap::list_roadmap`, `server_fns::roadmap::vote_roadmap_item`, and `server_fns::roadmap::RoadmapItem` (the whole `server_fns::roadmap` module);
- `server_fns::feedback::submit_feedback` and `server_fns::feedback::FeedbackSubmission`.

They returned empty results and nothing called them. Use the project feedback server functions instead: `portal_view`, `share_feedback`, and `portal_vote` in `server_fns::portal` for the public portal, and `feedback_console`, `feedback_action`, and `maintainer_reply` in `server_fns::feedback` for maintainers.

```rust
// before
let items = server_fns::roadmap::list_roadmap(repo_id).await?;

// after
let snapshot = server_fns::portal::portal_view(organization, project).await?;
let roadmap = snapshot.map(|snapshot| snapshot.feed.roadmap);
```

No migration guide applies: these functions were only reachable from the website's own pages, which the same release replaces.

#### Share accepted feedback as screened contributor tasks

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #801](https://github.com/monochange/monochange/pull/801)

Projects can opt into the experimental shared compute flow and share an accepted item as a task that contributors run with their own agents. The new `monochange_app_feedback::tasks` module holds the task model (`TaskDraft`, `TaskTarget`, `ContributorTask`, `PublicTask`, `TaskStatus`) and `screen_task`, the rules that run before a task lists. `FeedbackService` gains `set_shared_compute`, `task_repository`, `share_task`, `withdraw_task`, `open_tasks`, and `open_task`, and `FeedbackState` stores `shared_compute` and `tasks` (both default when an older document loads).

`monochange_app_api::feedback::share_task` pins a task to the tip of the issue repository's default branch through the new `github_app::default_branch_head`, and lists it for `TASK_LIFETIME` (14 days). Anyone can read open tasks without signing in:

```sh
curl https://monochange.dev/api/projects/acme/invoices/tasks
curl https://monochange.dev/api/projects/acme/invoices/tasks/task-1
```

The console calls the new `set_shared_compute`, `share_task`, and `withdraw_task` server functions.

**Breaking change:** `ConsoleView` gains `shared_compute`, and `ConsoleItem` gains `tasks` and `shareable`, so code that builds them with struct literals must set the new fields:

```rust
ConsoleItem {
	// ...existing fields
	tasks: Vec::new(),
	shareable: false,
}
```

#### Replace page components with lazy route types

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #796](https://github.com/monochange/monochange/pull/796)

The public page components (`BookPageView`, `ChangelogPage`, `DashboardPage`, `FeedbackConsolePage`, `InstallPage`, `OrganizationPage`, `PortalPage`, `PricingPage`, `ProjectPage`, and the private `AuthCallbackPage`) are gone. Each route is now a `LazyRoute` implementation (`BookRoute`, `ChangelogRoute`, `DashboardRoute`, `FeedbackConsoleRoute`, `InstallRoute`, `OrganizationRoute`, `PortalRoute`, `PricingRoute`, `ProjectRoute`, `AuthCallbackRoute`) whose `data()` creates the route's resources and whose `view()` renders them, referenced from `app.rs` through `Lazy::<Route>::new()`. Nothing outside this crate consumed the components; the website's rendered output is unchanged.

```rust
// Before
<Route path=path!("/dashboard") view=DashboardPage />

// After
#[lazy_route]
impl LazyRoute for DashboardRoute {
	fn data() -> Self { /* resources */ }
	fn view(this: Self) -> AnyView { /* renders this */ }
}

<Route path=path!("/dashboard") view={Lazy::<DashboardRoute>::new()} />
```

### 🚀 Feature

#### Build accepted feedback with the repository's agent

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #798](https://github.com/monochange/monochange/pull/798)

The maintainer console offers `ConsoleAction::BuildWithAgent` (`"build_with_agent"`) on accepted items whose GitHub issue names its repository. `monochange_app_api::feedback::start_agent_build(state, scope, item_id)` checks that the issue's repository has `AGENT_WORKFLOW` (`.github/workflows/monochange-feedback-build.yml`), sends it a `repository_dispatch` event named `AGENT_BUILD_EVENT` (`monochange-feedback-build`), and moves the item to building. A missing workflow returns the new `FeedbackError::AgentWorkflowMissing`, and a failed dispatch leaves the item accepted so it can be retried.

The event's `client_payload` is the new `monochange_app_feedback::BuildRequest`, ids only:

```json
{
	"item": "fb-12",
	"repository": "acme/web",
	"issue_number": 42,
	"branch": "fix/feedback-fb-12"
}
```

`github_app::repository_file_exists` and `github_app::dispatch_repository_event` are the new GitHub calls behind it; both use the Contents access hosted releases already need. The repository's workflow, which runs the agent with a read-only token and opens a draft pull request with the `Feedback-Item:` trailer from a separate job, is documented in the new "Build accepted feedback with an agent" guide.

#### Advance project feedback from pull request and release webhooks

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #788](https://github.com/monochange/monochange/pull/788)

`POST /api/github/webhooks` now handles `pull_request` and `release` events for every project that includes the event's repository:

| Event                                            | Effect                                                                                                                                                                                                                                          |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pull_request` `opened`, `reopened`, or `edited` | `FeedbackService::observe_pull_request` moves accepted or building items to review when the body closes their issue in the same repository (`Fixes #12`, `Fixes owner/name#12`, or an issue URL) or names them with a `Feedback-Item:` trailer. |
| `pull_request` `closed` with `merged: true`      | Calls `observe_merge`.                                                                                                                                                                                                                          |
| `release` `published` (not prereleases)          | Calls `ship_merged`. The version is the tag without a path prefix or leading `v` (`web/v1.2.0` → `1.2.0`); the notes link is the release page.                                                                                                  |

New public API:

- `monochange_app_feedback::closing_issue_numbers` and `feedback_trailers` parse pull request bodies.
- `IssueRef::is_in` checks an issue's repository.
- `monochange_app_api::feedback::Delivery` and `observe_delivery` apply an event to each affected project.

`monochange_app_api::feedback::update` no longer saves when an operation leaves the state unchanged and produces no notifications, so unrelated webhook traffic doesn't bump document versions.

Existing GitHub App registrations must subscribe to the `release` event. No new permission is required.

#### Use GitHub App user authorization for repository onboarding

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #782](https://github.com/monochange/monochange/pull/782)

Configure the GitHub App callback as `https://monochange.dev/auth/callback` and store its client ID and client secret in the `monochange_app` production profile. Login now uses PKCE and an exact callback URI, installation state is bound to the initiating browser, and expiring GitHub user tokens are refreshed from credentials stored by the new database migration.

#### Store organisations and projects for connected repositories

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #785](https://github.com/monochange/monochange/pull/785)

Database migration `004_add_organizations_and_projects` adds `provider` to `users`, `organizations`, `installations`, and `repositories`, adds `installations.organization_id` and `organizations.account_type`, and creates `projects` and `project_repositories`. Project repositories are keyed by `(provider, repository_external_id)` instead of `repositories.id`, so projects survive an uninstall and reinstall. Rolling the migration back leaves `installations.organization_id` in place because SQLite cannot drop a foreign-key column.

`installation` and `installation_repositories` webhooks now upsert the installation's account into `organizations` through `monochange_app_db::projects::link_installation_organization`. Installations recorded earlier are linked on first visit through the new `GitHubAppAuth::installation_account`, which reads `GET /app/installations/{id}` with the app JWT.

New server functions in `monochange_app::server_fns::organizations` (`list_organizations`, `organization_overview`, `create_project`, `project_overview`, `update_project`, `delete_project`) derive access from the repositories a user can manage, reusing the existing organisation-owner check. Routes `/dashboard/{organization}` and `/dashboard/{organization}/projects/{project}` render them; project forms use plain `ActionForm` fields such as `repositories[]`, so they work before hydration.

#### Store project feedback and expose the console and portal

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #786](https://github.com/monochange/monochange/pull/786)

Adds the `monochange_app_feedback` crate: the feedback pipeline with intake validation, prompt-injection screening, a `TriageEngine` seam with `RuleBasedTriage`, discussion and re-triage, voting, maintainer-only decisions, duplicate folding, and notifications. It also includes the disclosure rules (`Sensitivity::allowed_on(Surface, RepositoryVisibility)`) and the GitHub handoff drafts. `FeedbackService::from_state` and `state` round-trip a project's `FeedbackState`. Issue and pull request references now carry their repository, so merges and releases match by repository as well as number (`observe_merge(repository, number)` and `ship_merged`).

Database migration `005_create_project_feedback` adds `project_feedback`, one versioned JSON document per project, and `feedback_notifications`. `monochange_app_db::feedback::save_feedback` uses optimistic concurrency, and `monochange_app_api::feedback::update` reruns an operation on a lost race, up to three attempts. `monochange_app_api::feedback::create_issue` opens an issue through the GitHub App installation token with the new `github_app::create_issue`, then links it.

New server functions: `feedback_console`, `feedback_action`, and `maintainer_reply` for maintainers, plus `portal_view`, `share_feedback`, `portal_vote`, `portal_reply`, `similar_requests`, and `preview_markdown` for the public portal. Visitors are identified by a random `__Host-monochange_viewer` cookie, mapped to a per-project pseudonymous id with `feedback::viewer_id`. A project with a private or disconnected repository is treated as private.

#### Split the website client, compress the edge, and gate page performance

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #796](https://github.com/monochange/monochange/pull/796)

Release builds must now pass `cargo leptos build --release --split` (the Dockerfile and app CI do): every route except the landing and login views is a `#[lazy_route]` boundary, so the initial WASM download excludes dashboard, organisation, project, feedback, portal, book, changelog, install, and pricing code. Server functions referenced only by those views move into their route chunks, and the client hydrates through `leptos::mount::hydrate_lazy`, which lazy routes require.

Build and deployment changes:

- The WASM client compiles with the new `wasm-release` profile (`opt-level = "z"`, `panic = "abort"`, js-minify enabled) while the server binary stays on a runtime-first `release` profile (`opt-level = 3`).
- `strip` moved from the cargo profile into the Dockerfile: cargo's implicit `-C strip=debuginfo` corrupts proc-macro dylibs on newer macOS hosts, so profiles set `strip = "none"` and the image strips the server binary after linking. Proc-macros and build scripts build at `opt-level = 0` via `build-override`, which also speeds up cold builds.
- The Dockerfile uses BuildKit cache mounts for the cargo registry and target directory, so dependency artifacts survive between image builds.
- `/pkg` hydration assets are served `Cache-Control: public, max-age=0, must-revalidate` and the static file handler answers `If-Modified-Since` with 304s, so a deployment can never pair fresh HTML with stale client code while repeat visits revalidate for free. The image ships precompressed `.gz`/`.br` siblings of every pkg asset that the file handler serves to accepting clients, and the Caddy edge enables `encode zstd gzip` for everything else — production previously shipped every byte uncompressed.
- `app/deploy/verify-image.sh` also asserts the split build's manifest and that pkg assets are served gzip-compressed to accepting clients.

New website benchmarking, run by the `benchmark` job in `app-ci.yml` on every app-affecting change and locally through `pnpm bench:website` / `pnpm size:website`:

- `scripts/benchmark-website.ts` boots the release server against a throwaway SQLite database and runs Lighthouse (desktop preset) across the core flows, failing when metrics in `scripts/website-budgets.json` regress. `MONOCHANGE_ALLOW_BENCHMARK_BUDGET_FAILURES=true` keeps the CLI benchmark escape hatch.
- `scripts/website-size.ts` compares the gzip transfer size of `app/target/site/pkg` against the committed `scripts/website-size-baseline.json` and fails when the total grows beyond 1% (or a transferred file beyond 5%, absorbing cross-platform codegen variance) until the baseline is deliberately updated with `pnpm size:website:update`.

### 🐛 Fixed

- **Document the `--color` flag in the progress output reference.** The progress output reference page on the website now has a "Color" section that explains the global `--color <auto|always|never>` flag and the order in which `NO_COLOR`, `CLICOLOR_FORCE`, `CLICOLOR`, and `TERM=dumb` are checked under `auto`. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #795](https://github.com/monochange/monochange/pull/795)
- **Document the production GitHub App configuration.** The deployment guide now records the GitHub App permissions, webhook events, OAuth boundary, installation flow and production secret locations required to connect repositories and create commits safely. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #777](https://github.com/monochange/monochange/pull/777)

#### Warn at startup when the GitHub App is not configured

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #799](https://github.com/monochange/monochange/pull/799)

The app now logs a warning at startup when `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, and `GITHUB_APP_WEBHOOK_SECRET` are all unset. Before this change, a deployment without these values started silently and the dashboard only reported "Repository connection is unavailable", so operators had no server-side signal pointing at the missing credentials.

Configuration is unchanged. To enable repository connection in production, add the three values as fields of the `github` section in the `monochange.dev` item of the `monochange` vault and restart the app:

```bash
docker compose restart app
docker compose logs --tail=100 app
```

- **Explain which issues `CommentReleasedIssues` links.** The `CommentReleasedIssues` reference page now has a "Which issues are linked" section. It explains that only numbers GitHub resolves to issues in the release repository are linked, that pull requests, other repositories and code are skipped, and what happens when the lookup fails. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #797](https://github.com/monochange/monochange/pull/797) · _Closed issues:_ [#745](https://github.com/monochange/monochange/issues/745)

#### Resolve the hosted release API token through Monosecret

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #789](https://github.com/monochange/monochange/pull/789)

The hosted release endpoints (`POST /api/release-commits` and `POST /api/release-requests`) read `MONOCHANGE_TOKEN` straight from the process environment. It was the only release secret that bypassed the app's secret manifest, so production never supplied it from 1Password.

`MONOCHANGE_TOKEN` is now an optional password entry in `app/monosecret.toml`. The production profile reads it from the `release` section of the `monochange.dev` item, and it is loaded into `AppState::api_token`; an empty value counts as unset. Behavior is unchanged: tokens are compared in constant time, an unconfigured token answers `503`, and a wrong token answers `401`.

Operators who set `MONOCHANGE_TOKEN` as a plain environment variable on the server must add it as a `MONOCHANGE_TOKEN` password field in the `release` section of the `monochange.dev` item instead. Deployments that rely only on GitHub Actions OIDC need no change.

#### Refresh existing release branches from hosted release commits

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #789](https://github.com/monochange/monochange/pull/789)

`POST /api/release-commits` returned `409 Conflict` whenever the release branch existed at any commit other than the run's base commit. After the first run the branch always holds the previous release commit, so every later push failed until the release pull request was merged and its branch deleted.

The release branch is rebuilt from the base branch on every run, so the endpoint now replaces it with a forced ref update. Creating the branch or fast-forwarding it from `base_commit` is always allowed. A forced replacement is allowed only when all of these hold:

- the request names `base_branch` (CLIs before this release do not);
- the branch has the release-branch shape `<branch_prefix>/release`;
- the branch is neither `base_branch` nor the repository's default branch (read from `GET /repos/{owner}/{repo}` only when a force is needed);
- `base_branch` still points at `base_commit`.

When the base branch has moved, the run is stale and the endpoint returns `409` with a `base branch ... moved` message, leaving the release to the run for the newer commit. Any other refused replacement returns `409` with a `release branch ... moved` message, so a caller cannot force `main` or another non-release branch onto a different commit. The branch check runs before any blob, tree, or commit is written.

Both hosted endpoints now reject branch names that are not plausible Git refs with `400`: empty names, `..`, `//`, `@{`, a leading `/` or `-`, a trailing `/`, `.`, or `.lock`, control characters, and characters that Git forbids or that would change the API URL (`space ~ ^ : ? * [ \ # %`).

The endpoint also reported `verified: true` for every commit; it now returns GitHub's verification result.

- **Refresh shared ecosystem docs for Yarn lockfile support.** The shared documentation blocks on the website's guide pages and readme consumers now list Yarn workspaces and lockfiles alongside npm, pnpm, and Bun. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #778](https://github.com/monochange/monochange/pull/778) · _Closed issues:_ [#772](https://github.com/monochange/monochange/issues/772)

<details>
<summary><strong>🔨 Refactor</strong></summary>

#### Load monochange.dev secrets through Monosecret from one 1Password item

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #791](https://github.com/monochange/monochange/pull/791)

Production startup now reads every credential from a single 1Password item with one batched read, instead of one SecretSpec item per secret. This keeps the app well under the 1Password service-account request quota. Before deploying this release, operators must copy the production values into the new item and install the new Compose file.

`app/secretspec.toml` is replaced by `app/monosecret.toml`. The container selects its profile with `MONOSECRET_PROFILE` and no longer sets a provider:

```yaml
# before
environment:
  SECRETSPEC_PROFILE: production
  SECRETSPEC_PROVIDER: onepassword://monochange

# after
environment:
  MONOSECRET_PROFILE: production
```

The production profile reads the `monochange.dev` item in the `monochange` vault. Each secret is a field labelled with its name, in a section named after its group:

| Section   | Fields                                                                                                             |
| --------- | ------------------------------------------------------------------------------------------------------------------ |
| `auth`    | `JWT_SECRET`                                                                                                       |
| `github`  | `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, `GITHUB_APP_WEBHOOK_SECRET` |
| `release` | `MONOCHANGE_OIDC_AUDIENCE` (optional)                                                                              |
| `ai`      | `OPENROUTER_API_KEY` (optional)                                                                                    |

`DATABASE_URL` now comes from the Compose environment, and the bootstrap `OP_SERVICE_ACCOUNT_TOKEN` still comes from the Docker secret. The image keeps the bundled `op` CLI because Monosecret's 1Password provider shells out to it.

To migrate production:

1. Create the `monochange.dev` item with the sections and fields above, copying each value from its `secretspec/monochange_app/production/<KEY>` item.
2. Install `app/deploy/digitalocean/docker-compose.yml` from this release on the server before the image changes. The new image would treat a leftover `SECRETSPEC_PROVIDER` as an override and fail to start.
3. Deploy, then check `/health`, the app logs, sign-in, and the repository connection link.
4. Delete the old `secretspec/monochange_app/production/<KEY>` items.

Local development uses the `development` profile, which reads the ignored `app/.env`, then the environment, then local defaults, and never contacts 1Password. See "production cutover from SecretSpec to Monosecret" in `app/DEPLOY.md` for the full procedure and rollback.

#### Keep deployment credentials in the `monochange` 1Password vault

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #799](https://github.com/monochange/monochange/pull/799)

`app/deploy/digitalocean/monosecret.toml` now reads `DIGITALOCEAN_ACCESS_TOKEN` from the `deploy` item in the `monochange` vault, through the operator's own signed-in 1Password app. It no longer reads the local system keyring, and the separate `monochange deployment` recovery vault is retired. The same `deploy` item holds recovery copies of `WEBSITE_DEPLOY_SSH_KEY` and the read-only production `OP_SERVICE_ACCOUNT_TOKEN`, next to the app's `monochange.dev` runtime item, so every monochange credential lives in one vault.

Operators who kept the token in the keyring must add it to 1Password once:

| Item     | Field                       | Type     |
| -------- | --------------------------- | -------- |
| `deploy` | `DIGITALOCEAN_ACCESS_TOKEN` | password |

Commands are unchanged:

```bash
ms run --file app/deploy/digitalocean/monosecret.toml --reason "inspect production Droplet" -- devenv shell doctl compute droplet get 606211709
```

The app never reads the `deploy` item, but the production service account can, so keep the DigitalOcean token narrowly scoped and expiring. `app/DEPLOY.md` also now says the old SecretSpec items keep their value in a `password` field, not `value`.

- **Readability formatting across the workspace.** Apply monostyle's formatter-safe whitespace conventions and dprint normalization across every crate — blank-line separation around control flow and consistent formatting only, with no behavior changes. The repository now scores 84.5 overall on monostyle (readability 89.6) and gates future changes through `lint:monostyle`. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #793](https://github.com/monochange/monochange/pull/793)

</details>

## 0.2.3

### 🐛 Fixed

- **Allow release finalization to finish its cache cleanup.** Increase `release-post-merge.timeout-minutes` in `.github/workflows/ci.yml` from `10` to `20`. Pinned tooling setup, CLI compilation, release operations, and cache cleanup share this job deadline. The previous limit canceled release finalization while saving the Rust cache, after tags, draft releases, and the publish dispatch had succeeded. The larger budget leaves time for cleanup without changing release commands, permissions, or deployment checks. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #767](https://github.com/monochange/monochange/pull/767)

#### Use the maintainer identity for automated release pull requests

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #766](https://github.com/monochange/monochange/pull/766)

The repository's release workflow now opens release pull requests with the same existing maintainer token used to prepare their commits. Previously, the separate bot token made GitHub choose `github-actions[bot]` as the author of the queued squash commit and add the maintainer as a co-author.

The workflow now uses `RELEASE_PR_MERGE_TOKEN` for both operations. No new credential or permission is required. Existing bot-authored release pull requests must be replaced under the maintainer account before merging when sole maintainer authorship is required; changing the token does not change their author.

- **Verify repository changes through the dashboard.** The native SSR regression suite now delivers signed GitHub installation and repository-selection webhooks through the real API router, reloads the dashboard, and verifies additions, removals, suspension, uninstall and isolation between accounts. A second journey checks that organization repositories disappear when the installer loses ownership, and expired GitHub authorization shows a retry/sign-in state instead of an empty workspace. Only GitHub's external responses are replaced; cookies, SQLite, webhook signature verification and rendering stay real. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #771](https://github.com/monochange/monochange/pull/771)

## 0.2.2

### 🐛 Fixed

#### Choose release-note streams by audience for every package type

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #762](https://github.com/monochange/monochange/pull/762)

Generated release-agent instructions and the bundled skill now require agents to read configured stream and type descriptions and inspect output destinations before writing changesets. Application packages can use developer types for deployment or CI changes and product types for visible behavior. Agents create separate notes for the same package only when both audiences need them, then preview and render each output for review.

Use `monochange subagents <target> --force` to refresh an existing generated agent definition. Review local edits before replacing it. Inspect each audience with `monochange notes --output <id> --target <package>`; notes with a selected stream still need human or agent review of their prose.

The website now retains operational notes in `app/developer-changelog.md` instead of putting them in its public feed. Its implicit default changelog remains disabled. A named default-stream output retains those entries alongside the existing website outputs:

```toml
[changelog.outputs.website_developer]
stream = "default"
targets = ["monochange_app"]
path = "app/developer-changelog.md"
format = "keep_a_changelog"
mode = "append"
```

- **Resolve production deployment credentials in the environment-bound job.** The website deployment job now runs directly in `ci.yml` with the `website-production` environment, so its SSH key reaches the deployment step without crossing a reusable-workflow secret boundary. The job validates key presence and parsing before building the update and removes the temporary key after use. Operators receive an early error when deployment credentials are missing or malformed. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #759](https://github.com/monochange/monochange/pull/759)
