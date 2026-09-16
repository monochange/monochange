# Changelog

All notable changes to this crate are documented here. See [keep a changelog](https://keepachangelog.com/en/1.1.0/) for the format.

## monochange_classification [0.1.1](https://github.com/monochange/monochange/releases/tag/monochange_classification/v0.1.1) (2026-09-15)

### 🚀 Feature

#### Skip change classification on release pull requests and rename the `unknown` impact

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #709](https://github.com/monochange/monochange/pull/709)

## [0.1.0] - 2026-09-15

- Initial classification contract: compatibility findings, package decisions, and the skipped-report state.
