---
monochange: feat
---

# Switch release steps to the hosted backend with `MONOCHANGE_RELEASE_BACKEND`

`MONOCHANGE_RELEASE_BACKEND=hosted` moves every `CommitRelease` and `OpenReleaseRequest` step to the hosted backend without editing `monochange.toml`, so a release job can opt in while local runs and pull-request preflight jobs keep the configured backend:

```yaml
- run: monochange run release
  env:
    MONOCHANGE_RELEASE_BACKEND: hosted
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

A step input or command-line flag such as `--commit-backend local` still wins over the variable. `local` is also accepted, and any other value fails the step with an error naming the variable.
