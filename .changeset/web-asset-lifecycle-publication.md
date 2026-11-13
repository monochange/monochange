---
"monochange_app": patch
---

# Publish the web site only after both frontends build and keep manifests fresh

The leptoskit tooling plugin used to move each frontend's build output into the shared site tree inside `after_build` — which the leptoskit CLI runs before its bundle report and `bundle.budget_kib` check. A build the SDK's own budget rejected could already be visible at `app/target/site`, the SDK's report measured zero bytes because its files had been moved, and the merge-based assembly kept stale public files and chunks from earlier builds alive forever.

`build:web` now gives each build a fresh per-invocation scratch directory under the ignored `.monochange/local/web-build/<run>/`, so the SDK's report and budget always measure the real, complete scratch bundle. After BOTH builds succeed, `scripts/leptoskit-plugin/publish-web-site.ts` stages one deployment tree from those scratch outputs alone (both `pkg/` namespaces, the public assets, the WOFF2 fonts), verifies the two builds compiled byte-identical shared CSS, verifies every split manifest references only chunks that exist (and vice versa), precompresses the staged tree with upstream's gzip/brotli set, validates completeness, and promotes it over `app/target/site`. Any failure — build, validation, compression, or the promotion swap itself — leaves the last good published site byte-identical. The promotion is a two-rename directory swap; the window between the renames is documented in the publisher header (readers that open the site root in that instant see a missing directory), and the image copy only reads the tree after `build:web` exits.

`app/crates/monochange_app/src/frontend.rs` reads split manifests fresh on every non-deferred render instead of caching them in a process-global `Mutex`: a manifest that was missing, invalid, or from the previous generation now reaches the very next request after publication or repair, including same-sized edits no size-keyed cache could observe.

`app/deploy/verify-image.sh` requests the actual dual-namespace image layout — `/pkg/site/*` and `/pkg/projects/*`, the shared stylesheet, both manifests with every chunk they reference, the fonts and shell-referenced public assets, and both gzip and brotli precompressed responses — instead of the retired flat `/pkg/monochange_app.*` URLs.

Docker, CI, and the devenv task use the same build-and-publish sequence, and their assembled-site checks assert the real layout (no duplicate stylesheet in `pkg/projects`, fonts and precompressed twins present).

## Example

```bash
# Build both frontends into fresh scratch outputs, then publish one validated tree
devenv shell build:web --release
# Compare the published tree against the unchanged size baseline
pnpm node scripts/website-size.ts check --site-root app/target/site
```
