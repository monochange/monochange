# Version schemes and build numbers

**Status:** direction approved 2026-09-19, revised same day. **Implemented 2026-09-19** — see the "Implementation record" section at the end for what shipped and what remains. **Companion to:** [`distribution-targets.md`](./distribution-targets.md) — that plan's "Build number / binary identity" section is superseded by this document.

**Decision record.**

- **2026-09-19 (first).** Counter state lives in a tracked JSON file that monochange creates and owns; missing file = 0; stamp = +1.
- **2026-09-19 (revision, supersedes the storage decision).** No monochange-owned state file. Build numbers and any other stamped values live in **user-declared value files**: the user creates a file (e.g. `build.json` with `{"build": 0}`), and config points at a field inside it via a dotted path. Missing file or missing field is a **concrete error**, not a silent zero — the user creates the file. Calendar ordinals chain from the previous release record instead of any state file. Derived values (hashes, env, git, time) are stateless.

## Problem

`monochange` tracks exactly one version axis per release owner: a SemVer core. Real delivery targets need more:

- **App stores require a second, monotonic integer** — `CFBundleVersion` on Apple, `versionCode` on Google Play — with _different uniqueness scopes per store_ (verified: no `versionCode`/`build_number`/`CFBundleVersion` concept exists in any crate today).
- **Humans read versions that are not SemVer** — `2026.9`, `24.04`, `2026.1.3` — wanted as renderings, not as the planning identity, because bumps, ranges, and registries are SemVer.

The model: SemVer identity stays the ordering authority; everything else is a **value** injected into a template, where each value declares where it comes from and what happens to it at release.

## The three axes

| Axis           | What it is                  | Ordering authority           | State                                                                                                                 |
| -------------- | --------------------------- | ---------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| **Identity**   | SemVer core (`1.5.0`)       | Yes — the only authority     | Release record `versions` (exists today)                                                                              |
| **Counter**    | Monotonic integer (`45`)    | Within its reset policy      | **User-declared value file** (user-created, committed); reset policy keyed off the previous release record's identity |
| **Identifier** | Derived string (`a3f9b101`) | None — display material only | Stateless: computed at prepare, frozen into the record                                                                |

**One version per package.** The identity version exists only in the release record. Counter files store no version; the reset decision compares the new identity against the previous record. There is no place in the system where iOS and Android carry different app versions — they share the package identity, and only their _counter scopes_ differ.

### What exists today (verified in code)

- `Version` is `semver::Version` (`crates/monochange_core/src/lib.rs:86`); `BumpSeverity::apply_to_version` (`:226-265`) clears build metadata on every bump. Identity never carries `+N`; counters stay a separate axis.
- Template variables today: `{{ major }}`, `{{ minor }}`, `{{ patch }}`, `{{ version }}`, `{{ name }}`, `{{ ecosystem }}` — validated in `validate_floating_tag_template_variables` (`:1164`), rendered separately in `render_floating_tag` / `render_version_format_template` (`:1198`, `:1067`).
- `VersionedFileFormat` supports `json`, `toml`, `yaml`, `yml`, `env` (`:1722`). Flutter reads both store numbers from `pubspec.yaml` (`1.0.0+4`); Expo from `app.json` — both already supported. `plist`/`gradle` deferred.
- **Format-preserving field writes already exist**: the ecosystem crates update manifest fields by targeted text substitution (`update_manifest_text` in `monochange_dart`) rather than re-serialising, which preserves comments and ordering. Counter write-back reuses this approach — a naive re-serialise would destroy TOML/YAML comments in user files.
- `discover_release_record` (`crates/monochange/src/release_record.rs:56`) walks first-parent ancestry to the nearest record — this is the ordinal-chaining primitive, and it needs only the _immediately previous_ record, not an enumeration.

## Build number rules (researched)

**Apple, iOS: build numbers are scoped to the version string (the "release train"), not global.** Archived TN2420: "For iOS apps, build numbers must be unique within each release train, but they do not need to be unique across different release trains... you can use the same build numbers again in different release trains if you want to." So `1.0.0` (build 1) then `2.0.0` (build 1) is allowed.

**Apple, macOS: the opposite** — "build numbers must monotonically increase even across different versions."

**Google Play: global per app, never reusable** — "monotonically increase the value with each release, regardless of whether the release constitutes a major or minor release"; "You can't upload an APK to the Play Store with a versionCode you have already used for a previous version." Max `2,100,000,000`. `versionName` is free-form — the vanity label's home on Android.

| Rule                                                             | Status                                              |
| ---------------------------------------------------------------- | --------------------------------------------------- |
| iOS build unique per train; may reset in a new train             | Documented (archived TN2420)                        |
| macOS build globally monotonic                                   | Documented (TN2420 Important note)                  |
| `CFBundleVersion`: digits and periods, one to three components   | Documented, current page                            |
| Play `versionCode`: global, monotonic, unique, max 2,100,000,000 | Documented                                          |
| Rejected/expired build → number consumed, needs a higher one     | Documented for TestFlight; assembled for App Review |
| Shorebird release identity includes the build number (`1.0.0+1`) | Observed/enforced; docs silent                      |

Design consequence: the reset policy is per counter (`reset = "version"` for iOS-style trains, `reset = "never"` for Play/macOS-style), because the stores contradict each other.

## Versioning scheme survey (summary)

One law falls out of the survey:

> Anything monotonic by construction is either a **calendar stamp**, a **serial counter**, or a **well-ordered ordinal around a core**. Only calendar stamps and ordinals are pure functions of `(semver, date, ordinal)`; serials require persistent state.

This design maps directly onto it: calendar stamps and ordinals are **computed** (context variables and record-chained ordinals), serials are **counters** (user value files), and everything else is an **identifier** with no ordering guarantee. There is no fourth mechanism.

Registry finding — SemVer build metadata is not a carrier: PyPI rejects `+local` on upload, NuGet and Go strip it, Maven mis-parses `+`, npm/crates ignore it. Hence a separate axis. Calendar displays are compared **per series, never globally** (Ubuntu ships `24.04.3` after `25.04`). Scheme switches are new scheme ids, not template edits (Apple's 15 → 26).

## The model

### Configuration

```toml
# ── Reusable display schemes ────────────────────────────────────────────
[version_scheme.stamp]
template = "{{ year }}.{{ quarter }}.{{ release_of_quarter }}"

[version_scheme.short]
template = "{{ year_short }}.{{ month_padded }}"

# ── Declared values: each id becomes a template variable ────────────────
[package.app]

[package.app.values.build]
file = "build.json" # user-created, committed: {"build": 0}
field = "build"
on_release = "increment" # default; also "add" with amount, or "none"
reset = "version" # iOS-style train reset; "never" = Play-style

[package.app.values.play_code]
file = "build.json"
field = "play_code"
on_release = "increment"
reset = "never"

[package.app.values.artifact]
hash = "artifacts/app.aab" # read-only, derived
encoding = "base36" # hex | base32 | base36 | digits
length = 8

[package.app]
display_version = "stamp"

# ── Where values land ───────────────────────────────────────────────────
[[package.app.versioned_files]]
path = "pubspec.yaml"
value_template = "{{ identity }}+{{ build }}" # 1.5.0+4 (Shorebird identity)

[[package.app.versioned_files]]
path = "app.json" # Expo
value_template = "{{ identity }}+{{ play_code }}"
```

### Template variables

**Context variables** — computed, always available, no configuration:

| Group                                               | Variables                                                                                                                      |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Identity (existing, unchanged)                      | `major`, `minor`, `patch`, `version`, `name`, `ecosystem`                                                                      |
| Identity (new)                                      | `identity` (bare SemVer core), `prerelease`                                                                                    |
| Calendar (frozen prepare date, UTC)                 | `year` (2026), `year_short` (26), `month` (9), `month_padded` (09), `quarter` (1–4), `day`, `date` (20260919), `time` (153045) |
| Ordinals (chained from the previous release record) | `release_of_month`, `release_of_quarter`, `release_of_year`                                                                    |
| Composite                                           | `label` (the package's `display_version` rendering)                                                                            |

**Declared values** — each `[package.<id>.values.<vid>]` key becomes `{{ <vid> }}`. Value ids are validated against the reserved names above.

### Value sources and behaviours

| Source           | Config                                                                 | Read                                                                                                         | Stamped?                                                                            |
| ---------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------- |
| **File counter** | `file` + `field` (dotted path)                                         | Parsed per the file's format (`json`/`toml`/`yaml`/`yml` by extension); field must exist and hold an integer | Yes — written back by targeted text substitution so comments and formatting survive |
| **Hash**         | `hash` (path) + `algorithm` (default `sha256`) + `encoding` + `length` | Computed at prepare                                                                                          | No — read-only                                                                      |
| **Env**          | `env` (variable name)                                                  | Read at prepare                                                                                              | No                                                                                  |
| **Git**          | `git` (`short_hash` or `commit_count`)                                 | Derived from the release commit                                                                              | No                                                                                  |

Behaviours (`on_release`), valid for file counters only:

- `increment` (default) — value + 1; a fresh `{"build": 0}` first stamps to 1
- `add` with `amount = 10` — value + n, for teams that leave gaps between codes
- `none` — read-only pass-through of a counter the user manages elsewhere

Reset (`reset`):

- `never` — the counter only ever goes up (Play `versionCode`, macOS builds)
- `version` — resets to 1 when the identity version differs from the previous release record's (iOS release trains)

**Missing file or missing field is an error**, with a message naming the path, the field, and the expected shape. Empty files are not supported — the user creates the file with its starting value.

### Monotonicity classes and validation

| Class       | Members                                       | Guarantee                          | Template use              |
| ----------- | --------------------------------------------- | ---------------------------------- | ------------------------- |
| Calendar    | `year`…`time`                                 | Monotonic within their granularity | Anywhere                  |
| Ordinals    | `release_of_*`                                | Monotonic per series               | Anywhere                  |
| Counters    | file values with `increment`/`add`            | Monotonic within reset policy      | Anywhere                  |
| Identifiers | hashes, `env`, git, `time`-derived composites | **None** — unique-ish, not ordered | Display material; flagged |

A scheme whose template uses identifier variables is marked `ordering = "none"`: per-series monotonicity checks are skipped with a warning, and rendered-label **collision** against the package's previous labels remains a hard error (two releases in one month must never render the same string). This is the precise answer to "it probably needs to be a number that only goes up": ordering slots want counters or ordinals; hashes are legal but surrender the ordering guarantee, and monochange says so instead of silently pretending.

### Ordinal chaining

`release_of_month`/`_quarter`/`_year` chain from the **previous release record's** frozen `label_inputs`: same calendar period as the previous release of this owner → previous count + 1, otherwise 1. First release of an owner → 1. No state file, no enumeration walk — `discover_release_record` already finds the immediately previous record in one ancestry step. Note the adoption caveat: ordinals count from the first monochange release onward, which is documented rather than corrected.

### Idempotency and the resubmission contract

Unchanged from the earlier decision: if `.monochange/releases/<hash>/release.json` already exists for `(id, kind, version)` — a direct lookup, since record paths hash over exactly that — its frozen `values`/`labels` are reused and counters are **not** re-stamped. Otherwise counters stamp and files are written. The App Store reject→resubmission loop (distribution plan) is later built as an explicit amend-and-recommit that increments the `reset = "version"` counter; plain re-prepare is always a no-op for counters.

### Schema wiring

- **Config schema**: the new tables flow into the generated `monochange.schema.json` automatically (`schemars` on the raw config types); regenerate via `xtask schema update`, keep `additionalProperties: false`, and keep `config_schema_covers_current_root_toml_top_level_keys` green.
- **Release-record schema**: `0.7` → `0.8`, adding `values`, `labels`, `label_inputs` with `#[serde(default)]` and a migration that injects empty objects for historical records. Versioned schema artifacts regenerated.
- **User value files: no schema by design.** They are user-owned documents with one declared integer field; validation is parse-plus-extract with concrete errors. Being non-prescriptive here is the point.

## Consumption points

| Surface                                       | Today                             | Gains                                             |
| --------------------------------------------- | --------------------------------- | ------------------------------------------------- |
| Release tags (`version_format`, `render_tag`) | major/minor/patch/version         | full namespace                                    |
| `floating_tags`                               | components only (no full version) | calendar components (`v{{ year }}.{{ quarter }}`) |
| `release_title`, `changelog_version_title`    | version-based                     | `{{ label }}`, declared values                    |
| `[changelog.outputs.<id>].path`               | `{{ version }}`                   | full namespace (per-year changelog files)         |
| `versioned_files` values                      | identity + prefix                 | `value_template`                                  |
| Release record / manifest JSON                | `versions`                        | `values`, `labels`, `label_inputs` frozen         |
| `monochange notes`                            | version strings                   | values and labels alongside                       |
| `[distribution.<id>]` (later)                 | —                                 | counter references, `display_version`             |

Freezing matters: a release prepared on 30 September renders the same string in October, because consumers read the record, never the calendar or the files.

## Validation rules

1. **Identity is a bare SemVer core.** `value_template` on an identity-bearing manifest path rejects calendar, ordinal, and declared-value variables; packages whose manifest must carry a non-SemVer version use `version_source = "tag"` (exists today).
2. **Counter files must exist with an integer field** — missing file, missing field, or non-integer value is a blocking error naming path, field, and expected shape.
3. **Reserved ids**: declared value ids must not collide with context variables.
4. **Per-series label monotonicity** for ordering-capable schemes; `ordering = "none"` schemes skip it with a warning. Rendered-label collision is always a hard error.
5. **Bounds guidance**: Play ceiling 2,100,000,000; date-based codes collide with it mid-2033; Apple `CFBundleVersion` renders digits and periods, one to three components.
6. **PyPI-targeted identity files never emit `+` values** — local versions are rejected on upload.
7. **Scheme template changes between releases are flagged** — switch schemes by adding a new scheme id.

## Implementation plan

Each PR is shippable, keeps patch coverage at 100%, and follows the test-layout rules (`__tests__/<module>_tests.rs`; integration tests in `crates/monochange_integration_tests` with file fixtures and Insta snapshots, multiline JSON redacted).

### PR 1 — `feat(core): value sources, behaviours, and scheme domain types`

Pure domain, no wiring.

- `crates/monochange_core/src/versioning.rs`; tests at `src/__tests__/versioning_tests.rs`
- Types: `ValueSource` (file/hash/env/git variants, `untagged` serde with disjoint fields), `StampBehaviour` (`increment` | `add{amount}` | `none`), `ResetPolicy` (`never` | `version`), `ValueDefinition`, `VersionSchemeDefinition`, `LabelInputs { date, of_month, of_quarter, of_year }`, `ValueSnapshot`
- Hash encodings over `sha256`: `hex`, `base32`, `base36`, `digits` (numeric-only, truncated to `length`) with test vectors
- `chain_label_inputs(previous: Option<&LabelInputs>, date) -> LabelInputs` (period rollover for month/quarter/year, including January rolling all three)
- `stamp_counter(behaviour, current, identity_changed) -> u64`
- Counter-file read: format by extension, dotted-field extraction, integer enforcement — the error type carries path + field + expected shape
- Tests: fresh `0 → 1`; increment; add; `reset = "version"` on change and not; missing file/field/non-integer errors; encoding vectors; chaining vectors; serde round-trips; `deny_unknown_fields`

### PR 2 — `feat(config): parse version schemes, declared values, and value templates`

- `RawPackageDefinition` gains `values: BTreeMap<String, RawValueDefinition>` and `display_version: Option<String>`; `RawWorkspaceConfiguration` gains `version_scheme: BTreeMap<String, RawVersionScheme>` (additive under root `deny_unknown_fields`); `VersionedFileDefinition` gains `value_template: Option<String>`
- Validation: scheme keys and value ids are lowercase identifiers; value ids avoid reserved names; templates use only available variables (context ∪ the package's declared ids); `display_version` references a declared scheme; `value_template` axis references exist; identity-file rule 1 above
- `xtask schema update`; update the schema-coverage integration test; annotated commented blocks in `crates/monochange/src/monochange.toml.template` for every new key (product rule)
- Tests in `crates/monochange_config/src/__tests__/versioning_tests.rs`

### PR 3 — `feat(release): stamp values and labels during prepare-release` (includes record schema 0.8)

The behavior change; record fields and schema version move together.

- Hook into the prepare-release versioned-files phase:
  1. Idempotency guard: existing record for `(id, kind, version)` → reuse frozen values, stamp nothing
  2. Read declared sources; stamp file counters with targeted, format-preserving write-back (the `update_manifest_text` approach, not re-serialisation)
  3. Chain `label_inputs` from the previous record (`discover_release_record`); render schemes
  4. Render `value_template`s; write versioned files; the stamped counter files appear in the release diff (`build: 3 → 4` is reviewable)
  5. Freeze `values`, `labels`, `label_inputs` into `ReleaseManifest` and `ReleaseRecord`
- Dry-run computes and reports without writing
- Packages with no values and no `display_version`: zero behavior change, no files touched (tested)
- `crates/monochange_schema/SCHEMA_VERSION` → `0.8`; migration `release_record_0_7_to_0_8.rs`; regenerate schemas and artifacts; repo-wide `snapshot:update`
- Integration fixtures under `fixtures/tests/version-values/`: first-stamp; increment; version reset vs never; missing-file error; hash value; ordinal rollover; re-prepare idempotent; no-values unchanged; value-template write

### PR 4 — `feat(core): unified version template renderer`

- One `render_version_template(template, ctx, surface)` + validator, `VersionTemplateSurface { Tag, FloatingTag, Title, Path, Value }` with per-surface profiles (`FloatingTag` keeps its no-full-version rule); refactor the three existing renderers onto it
- Wire `release_title`, `changelog_version_title`, `render_named_output_path` (changelog crate), and `value_template` rendering to the shared renderer with the full variable namespace
- Regression tests: existing templates render identically; per-surface rejections

### PR 5 — `docs(versioning): document values, schemes, and counter files`

- Guide (`04-configuration.md`, `06-release-planning.md`): new keys, the counter-file contract (user-created, integer field, committed), the monotonicity classes, the adoption recipe
- Reference for hash encodings and reserved variable names; `mdt` blocks where README/guide overlap
- Two changesets per stream rules: `default`-stream (config/API detail, migration) + `user`-stream (visible outcome), no duplicated prose

### Validation (every PR)

```bash
devenv shell fix:all
devenv shell build:all
devenv shell lint:all
devenv shell test:all
devenv shell docs:update
devenv shell docs:check
devenv shell coverage:patch
devenv shell monochange step validate
```

Plus `snapshot:update` where snapshots change and `cargo semver-checks` against `origin/main` to size changeset bumps.

## Deliberately deferred

- **Distribution wiring** (`[distribution.*]`) — gated on the distribution-targets plan.
- **`plist` / `gradle` versioned-file formats** — Flutter (`pubspec.yaml`) and Expo (`app.json`) are covered today.
- **Remote counter sources** (EAS-style) — user files and git are local by design.
- **Attempt/resubmission step** — ships with the distribution plan; the idempotency contract here is forward-compatible with it.
- **More hash algorithms** — `sha256` only; the encoding surface (hex/base32/base36/digits) is where the variability lives.

## Decisions

| # | Decision                         | Outcome                                                                                                                              |
| - | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| 1 | Counter storage                  | User-declared files with dotted-field extraction; user creates the file; missing file/field = error. No monochange-owned state file. |
| 2 | Behaviours                       | `increment` (default), `add(n)`, `none`; reset `never` / `version`                                                                   |
| 3 | Derived values                   | `hash` (sha256 → hex/base32/base36/digits), `env`, `git` — read-only, flagged non-monotonic                                          |
| 4 | Ordinals                         | Chained from the previous release record; no state file                                                                              |
| 5 | One version                      | Identity lives only in the release record; counter files store no version; iOS/Android differ only in counter reset policy           |
| 6 | Variables                        | Calendar incl. `quarter` and `time`; ordinals incl. `release_of_quarter`; declared ids become variables                              |
| 7 | Schemas                          | Config + release-record schemas generated/migrated as usual; no schema for user value files                                          |
| 8 | Re-prepare of an existing record | No-op for counters; resubmission is an explicit later action                                                                         |

## Risks

- **User counter files are hand-editable.** Mitigated by integer enforcement, concrete errors, and readiness cross-checks against the previous record's frozen values.
- **Format-preserving write-back is the fiddly part.** Targeted substitution must handle nested dotted fields in three formats; covered by table-driven tests with comment-bearing fixtures.
- **Merge conflicts on counter files** across parallel release branches: take either side, re-run prepare; the record guard prevents double-stamping.
- **Counter file deleted** — prepare fails loudly (by design, since silent zeros would make stores reject duplicates); recovery is git history.
- **Identifier variables tempt users into non-monotonic labels.** Mitigated by the `ordering = "none"` warning and the hard collision check.
- **Two renderers drifting** during the PR 2 → PR 4 window — mitigated by the ordering note and regression tests.

## Implementation record

Implemented on 2026-09-19. All repository gates pass: 3599 tests, clippy clean with `-D warnings`, 100% patch coverage, `monochange step validate`, `xtask schema check`, and the snapshot-readability check.

### What shipped

- **Value sources and behaviours** (`crates/monochange_core/src/versioning.rs`): `file` counters with dotted-field paths, `hash` with `hex`/`base32`/`base36`/`digits` encodings, `env`, `git` (`short_hash`, `commit_count`), and `timestamp` (`now`, `commit`). Stamping supports `increment`, `add`, and `none`, with `reset = "never" | "version"`.
- **Template namespace**: `identity`, `prerelease`, `year`, `year_short`, `month`, `month_padded`, `quarter`, `day`, `date`, `time`, `release_of_month`, `release_of_quarter`, `release_of_year`, `label`, plus every declared value id. Rendering is longest-name-first so `build.android` cannot be mangled by a shorter `build`.
- **Version schemes** (`[version_scheme.<id>]`) referenced by `[package.<id>].display_version`, rendered through the shared template renderer.
- **Ordinal chaining** from the previous release record, restarting in a new month, quarter, or year.
- **Counter write-back** that rewrites only the declared field, preserving surrounding formatting and comments.
- **Idempotency guard**: a release whose record already exists reuses its frozen values instead of advancing counters.
- **Frozen `values`, `labels`, and `label_inputs`** in `ReleaseManifest` and `ReleaseRecord`, omitted when nothing was rendered so existing manifests are byte-identical.
- **`value_template`** on versioned files, with validation rejecting non-`SemVer` variables on a package's own ecosystem manifest.
- **Config parsing and validation**, the annotated `monochange init` template, regenerated schemas, a guide section, and two changesets.

### Corrections to this plan's assumptions

- **No schema version bump was needed.** The versioned schema filename derives from the `monochange_schema` package version plus the planned changeset bump, and schema 0.7 is unreleased (not reachable from tag `v0.13.0`). The new release-record fields therefore land in 0.7 and no `release_record_0_7_to_0_8` migration exists. A future bump adds one edge.
- **The release plan is keyed by discovery record id**, not config id. Values resolve per config id, so the released-version map is re-keyed through each package's `config_id` metadata. The first implementation looked up by config id against a record-id-keyed map, which surfaced only when running the CLI against a real fixture — unit tests alone did not catch it.
- **`PreparedRelease.versioning` widened `ResolvedReleaseValues` to `pub`** because the field is public on a public struct.

### Deliberately not implemented

- **`plist` and `gradle` versioned-file formats.** Flutter reads both store numbers from `pubspec.yaml` and Expo from `app.json`; both formats already work. Add when a project needs raw Xcode or Gradle files.
- **Distribution wiring** (`[distribution.<id>]`) — gated on [`distribution-targets.md`](./distribution-targets.md).
- **The App Store resubmission step.** The idempotency guard is the forward-compatible half; an explicit amend-and-recommit action ships with the distribution plan.
- **Alternative counter sources** (remote/EAS-style, `git_commit_count` as a primary counter). The file and derived sources cover the shipped use cases.
- **Per-series label monotonicity warnings and rendered-label collision detection.** Validation currently enforces the structural rules (declared schemes, available variables, manifest `SemVer`); runtime ordering checks against the previous release's labels are not yet implemented.

### Test coverage

- `crates/monochange_integration_tests/tests/release_values.rs` — 13 CLI-level tests over 8 file fixtures under `fixtures/tests/versioning/`, asserting with Insta snapshots. Dates are pinned with `MONOCHANGE_RELEASE_DATE` so calendar labels and ordinals are deterministic.
- Fixtures: `counter-train`, `counter-owner`, `value-template`, `display-label`, `ordinal-chain`, `derived-sources`, `missing-counter`, `existing-record`.
- Scenarios: train reset vs owner increment, re-prepare idempotency, two counters on one app reaching different files, calendar and quarterly schemes, monthly ordinal chaining with a month rollover, hash/env/timestamp values, missing counter file, missing environment variable, dry-run leaving files untouched, frozen record values.
- Unit tests: `monochange_core/src/__tests__/versioning_tests.rs` (54), `monochange_config/src/__tests__/versioning_tests.rs` (31), `monochange/src/__tests__/versioning_state_tests.rs` (41).

**Manifest validation was corrected by this work.** The first implementation rejected any non-`SemVer`-name variable on a package manifest, which broke Flutter: `pubspec.yaml` carries `1.2.3+4`, and that _is_ valid `SemVer` build metadata — the entire Shorebird release-identity case. Validation now renders the template and parses the result, so a counter appended to the identity is accepted while calendar versions and letter-bearing values in numeric positions are still rejected. This was found by running the CLI against a real fixture, not by unit tests.

**Coverage note.** `llvm-cov` attributes zero-count regions to lines that contain no statements — doc comments, blank lines, and the parameter lines of a function signature. The patch gate counts any line present in the LCOV record, so those lines were marked with the repository's existing `// patch-coverage:ignore-start` / `ignore-end` convention, each with a reason. The real fix was restructuring the error branches so each line is a statement, which removed most of the artifacts: multi-line `config_diagnostic(...)` calls were collapsed into short calls with a pre-built `labels` vector, because `rustfmt` re-wraps long calls and reintroduces delimiters on their own lines.
