---
"monochange": feat
"monochange_config": patch
"monochange_graph": patch
"monochange_github": patch
---

# Show warnings without `--log-level`

Warnings raised with `tracing::warn!` were only visible with `--log-level`, so CI had to run `monochange --log-level=debug` to notice problems such as the release pull request silently falling back from a verified GitHub API commit to a regular git commit. monochange now prints every warning by default as one readable line with its details underneath, through the same stderr channel as progress, so it never splices into an active spinner:

```text
warning: could not create a verified release commit through the GitHub API; falling back to a regular git commit
  reason: GitHub API POST `/repos/acme/app/git/trees` failed: status 422
  commit: c686e78478ea2611d41e2d8521311a236f2a3470
```

Each distinct warning prints once per run. `--quiet` hides warnings, GitHub Actions receives them as `::warning` annotations, and `--progress-format json` emits a `warning` event with `message` and `fields`. `--log-level` still enables the full maintainer trace.

`monochange_config` (non-standard GitHub `[source]` host) and `monochange_graph` (a version group member missing from discovery) now report their warnings with `tracing::warn!` instead of `eprintln!`, so they respect `--quiet` and progress rendering. Embedders that relied on those lines reaching stderr without a tracing subscriber must install one. Routine internal messages ("ignoring stale prepared release artifact", a failed prepared-release cache save, and git failures that are already returned as errors) moved to `debug`.
