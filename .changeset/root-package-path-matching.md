---
"monochange_core": patch
---

# Match repository-root packages configured with a dot path

A package declared as `path = "."` — the documented form for GitHub Actions repositories and other single-package repos — was never matched by `PackagePathMatcher`, so `monochange step affected-packages` reported no affected packages for changes inside it. Changeset policy therefore passed silently instead of requiring a changeset.

The matcher now normalizes a `"."` (and `"./"`) package path to the repository root prefix, so every repository path belongs to that package. Other package path forms are unaffected.

```toml
[package.actions]
path = "."
type = "github_actions"
```

With this fix, editing `src/actions/merge/index.ts` in that repository reports `actions` as affected and fails verification until a `.changeset/*.md` entry covers it.
