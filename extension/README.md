# monochange browser extension

leptoskit extension scaffold for glancing at **projects** and **release status** beside any tab. Chrome follows the Release floor design tokens (`--mc-*`) from the landing design system.

```text
extension/
├── leptoskit.toml                 # platforms = ["extension"]
├── assets/app.css                 # --mc-* chrome
├── monochange_extension_shared/   # RPC stubs (swap when web/RPC port merges)
└── monochange_extension_app/      # popup, options, background worker
```

## Surfaces

| Surface    | Route      | Role                                      |
| ---------- | ---------- | ----------------------------------------- |
| Popup      | `/`        | Brand chrome, sign-in, projects, releases |
| Options    | `/sign-in` | GitHub sign-in entry                      |
| Background | —          | Session refresh messages                  |

## RPC

Until the leptoskit web/RPC port publishes shared types, `monochange_extension_shared` stubs the documented shapes from today's server functions:

- `GET /rpc/session` → `Option<SessionUser>`
- `POST /rpc/login-url` → `{ "url": "…" }`
- `POST /rpc/logout`
- `GET /rpc/organizations` → `Vec<OrganizationSummary>`
- `GET /rpc/website-releases` → `Vec<WebsiteRelease>`

The HTTP client is bearer-oriented (extension storage). Replace the stub module with a path dependency on the web shared crate when that PR merges; keep `HttpRpcClient` or switch callers to `leptoskit::rpc::client::call`.

## Develop

From the repository root, use the devenv shell:

```bash
devenv shell
cd extension
cargo test -p monochange_extension_shared -p monochange_extension_app
```

Package for Chromium and Firefox (requires the upstream `leptoskit` CLI):

```bash
# from the repository root
devenv shell cargo bin --install
export PATH="$PWD/.bin/rust-nightly/leptoskit_cli/0.3.0/bin:$PATH"
cd extension
leptoskit build --platform extension
# → .leptoskit/extension/chromium and firefox
leptoskit dev --platform extension
```

Point `[extension].server` in `leptoskit.toml` at a local app (`http://127.0.0.1:3000`) while developing against a running backend that exposes the `/rpc/*` routes.

## Design

Release floor tokens live in `leptoskit.toml` `[theme]` (and `chrome::THEME_*`). Popup/options UI uses **Tailwind utility classes** with `var(--mc-*)` — not semantic `.mc-*` CSS sheets. `assets/app.css` only carries fonts and document base styles.

Brand lockup: mark asset + lowercase **monochange** wordmark text in the compact header (full SVG wordmark remains on the website).

## Lint

```bash
leptoskit lint --deny-warnings --no-clippy
```

`[lint]` denies `component_attribute` and `markup_style` (`markup = "view"`).
