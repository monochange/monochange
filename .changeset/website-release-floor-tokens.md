---
monochange_app: website_feature
---

# Refresh the website to the Release floor look

Visitors see a mist/graphite page with teal actions instead of the old violet/indigo marketing chrome, and the homepage opens with the brand before any demo.

- **Brand-first hero** — Full-bleed dark hero with the mark and wordmark, one headline (“Ship every package from one floor.”), one supporting sentence, and CTAs to open the workspace or install the CLI. The interactive release-plan demo moves below the fold.
- **Product floor** — Linked Projects, Releases, Feedback, and Org/dashboard rows sit under the hero so the site reads as a workspace product, not only a CLI pitch.
- **Shared `--mc-*` tokens** — Light/dark CSS custom properties (`--mc-page`, `--mc-accent`, `--mc-hero`, …) and Tailwind `@theme` colors drive chrome and atmosphere; landing/nav/footer compose with utilities instead of new semantic CSS class stacks.
- **Lint-ready components** — Homepage sections use `#[component]` + `view!` with PascalCase names for leptoskit component lints.
