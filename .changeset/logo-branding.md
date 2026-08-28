---
monochange:
  bump: patch
  type: docs
monochange_analysis: docs
monochange_cargo: docs
monochange_changelog: docs
monochange_config: docs
monochange_core: docs
monochange_dart: docs
monochange_deno: docs
monochange_ecmascript: docs
monochange_forgejo: docs
monochange_gitea: docs
monochange_github: docs
monochange_gitlab: docs
monochange_go: docs
monochange_graph: docs
monochange_hosting: docs
monochange_lint: docs
monochange_linting: docs
monochange_npm: docs
monochange_publish: docs
monochange_python: docs
monochange_schema: docs
monochange_semver: docs
monochange_snapshot: docs
monochange_telemetry: docs
monochange_test_helpers: docs
---

# add the monochange logo across readme, docs.rs, and the mdBook

Every published crate now renders the monochange mark on docs.rs through `html_logo_url`, and docs.rs pages use the matching favicon through `html_favicon_url`. The mark itself is a chunky lowercase `mc` monogram with a version-bump arrow in the negative space.

- the readme gains a top-level hero logo that follows the reader's theme: a light variant on light GitHub themes and a light-on-dark variant on dark themes, using the `picture` element with `prefers-color-scheme`
- the mdBook in `docs/` picks up a new `favicon.png`
- `assets/` holds the exported logo sizes (280, 512, 1024), the dark variant, and a multi-size `favicon.ico`
- a reserve mark (the navy Converge badge) is kept under `assets/reserve/` for a future rebrand
