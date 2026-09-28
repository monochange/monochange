# Changelog

All notable changes to this crate are documented here. See [keep a changelog](https://keepachangelog.com/en/1.1.0/) for the format.

## monochange_classification [0.2.0](https://github.com/monochange/monochange/releases/tag/monochange_classification/v0.2.0) (2026-09-19)

### 💥 Breaking Change

#### Report a break against main separately from the release verdict

- `decision.release_impact` reports the compatibility impact measured between the package's latest release and the candidate. A pull request that only changes an API the latest release never contained now reads `compatibility_impact: breaking` with `release_impact: additive`.
- `decision.proposed_changeset_bump` and `decision.enforceable_minimum` are capped by the release comparison, so a modeled finding can no longer propose a bump higher than the release-relative bump for the same package. Nobody holding the latest release can observe a break in an item that the release comparison does not show as changed.
- Unmodeled findings stay uncapped. They are the safety floor for a surface the analyzers cannot model, and the release comparison cannot refute them.
- `decision.release_floor` reports the accumulated unreleased bump without inheriting a break that only exists against the default branch.
- The classification report contract advances to `schema_version` `0.2`: `decision.release_impact` is new, and `decision.proposed_changeset_bump`, `decision.enforceable_minimum`, and `decision.release_floor` can be lower than in `0.1` for the same pull request.

```json
{
	"compatibility_impact": "breaking",
	"release_impact": "additive",
	"proposed_changeset_bump": "minor",
	"release_floor": "minor"
}
```

No configuration change is required. Re-run `monochange change classify` to pick up the release-relative verdict.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #712](https://github.com/monochange/monochange/pull/712)

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

### 🐛 Fixed

#### Accept `monochange_snapshot::SnapshotView` in the public snapshot helper

`monochange_classification` publicly depends on `monochange_snapshot`, and `run_package_snapshot` exposes a type from it directly:

```rust
pub fn run_package_snapshot(
	root: &Path,
	package_id: &str,
	save: bool,
	view: monochange_snapshot::SnapshotView,
) -> MonochangeResult<String>
```

`SnapshotView` is not re-exported from this crate, so callers must depend on `monochange_snapshot` to name the argument. That makes `monochange_snapshot` part of this package's consumer-visible surface rather than a private implementation detail: a breaking change to `SnapshotView` reaches callers of this function without any change here.

`monochange_snapshot` gained an opt-in `schema` feature and is released as `0.1.1`, so this changeset records the coupling for review. No signature changed and no caller needs to update anything. Pass `monochange_snapshot::SnapshotView::Index` for the compact surface used by the release workflow, or re-export the enum and re-classify once the analyzer models re-exports precisely.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #711](https://github.com/monochange/monochange/pull/711) · _Related issues:_ [#705](https://github.com/monochange/monochange/issues/705)

## monochange_classification [0.3.0](https://github.com/monochange/monochange/releases/tag/monochange_classification/v0.3.0) (2026-09-28)

### 💥 Breaking Change

#### Scope the classified bump to the pull request

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #730](https://github.com/monochange/monochange/pull/730) · _Closed issues:_ [#706](https://github.com/monochange/monochange/issues/706)

- `decision.pull_request_changes` reports whether this contribution touches the package. When it is `false`, every finding came from the `release` or `releaseToDefault` interval, so `decision.proposed_changeset_bump` and `decision.enforceable_minimum` are `none`, `decision.review_required` is `false`, and the accumulated change appears only in `decision.release_floor` and `decision.release_impact`. The net candidate and the local working tree set the flag; the release comparisons never do.
- A pending changeset for a package the pull request does not modify still reports `action: review` with the summary `the pull request does not change this package; the pending changeset intent needs review`, because a changeset can intentionally describe a consumer-facing effect implemented in another package. It no longer escalates the bump or the review verdict for work an earlier merge introduced.
- The Cargo analyzer classifies a pure append to a public `const`/`static` whose declared type is `&[T]`, `[T; N]`, or `Vec<T>` and whose initializer is a literal as `additive`/`minor` with high confidence, matching `cargo semver-checks`. A removal, a reorder, an element edit, a changed element type, or a non-literal initializer stays conservative.

```json
{
	"compatibility_impact": "compatible",
	"release_impact": "breaking",
	"pull_request_changes": false,
	"proposed_changeset_bump": "none",
	"release_floor": "major",
	"review_required": false
}
```

The classification report contract advances to `schema_version` `0.3` (`SCHEMA_VERSION` regenerated with a frozen `classification.v0.3.schema.json`; the shipped v0.1 and v0.2 assets are untouched, and the schemas reference page lists the new asset). `decision.pull_request_changes` is new, and `decision.proposed_changeset_bump`, `decision.enforceable_minimum`, and `decision.review_required` can be lower for a package this pull request does not touch.

No configuration change is required. Re-run `monochange change classify` to pick up the pull-request-scoped verdict.

## [0.1.0] - 2026-09-15

- Initial classification contract: compatibility findings, package decisions, and the skipped-report state.
