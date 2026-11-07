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

Package for Chromium and Firefox (requires `leptoskit` CLI 0.3):

```bash
cargo install leptoskit_cli --version 0.3.0 --locked
cd extension
leptoskit build --platform extension
# → .leptoskit/extension/chromium and firefox
leptoskit dev --platform extension
```

Point `[extension].server` in `leptoskit.toml` at a local app (`http://127.0.0.1:3000`) while developing against a running backend that exposes the `/rpc/*` routes.

## Design

Popup chrome uses the shared CSS variables:

`--mc-page`, `--mc-surface`, `--mc-ink`, `--mc-muted`, `--mc-rule`, `--mc-accent`, `--mc-accent-deep`, `--mc-accent-soft`, `--mc-signal`, and the control/panel radii.

Brand lockup: mark asset + lowercase **monochange** wordmark text in the compact header (full SVG wordmark remains on the website).
