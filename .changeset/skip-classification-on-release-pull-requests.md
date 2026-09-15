---
"monochange": minor
"monochange_core": minor
"monochange_config": minor
monochange_schema: major
monochange_classification: patch
---

# Skip change classification on release pull requests and rename the `unknown` impact

- New crate `monochange_classification` owns the classification report contract and its schema, versioned independently of the release train.

- `monochange change classify` accepts `--label` and reads `[changesets.classification].skip_labels` (default `["release"]`). A matching label reports `skipped: true`, analyzes no packages, and exits successfully, so the release pull request monochange opens is no longer classified.
- The `unknown` compatibility impact is now `unmodeled`. The change is still outside the analyzer's modeled public surface, but the package itself is supported, so the previous name overstated how much was unknown.
- The `change-classification` GitHub Action gained a `labels` input and defaults it to the current pull request's labels. A skipped run deletes any comment left from an earlier revision.

```toml
[changesets.classification]
# Set to [] to classify every pull request.
skip_labels = ["release"]
```

```bash
monochange change classify --format json --label release
```

The published configuration contract gained `[changesets.classification]`, so the schemas advance to `v0.7`; the `0.6` → `0.7` migration edge accepts existing release records unchanged.
