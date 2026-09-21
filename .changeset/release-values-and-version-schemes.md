---
"monochange": major
"monochange_core": major
"monochange_config": minor
"monochange_schema": minor
"monochange_github": patch
---

# Declare release values and version schemes for counters and calendar labels

Release planning tracked one version axis per release owner: a `SemVer` core. Delivery targets that need a second monotonic number (Apple `CFBundleVersion`, Google Play `versionCode`) or a human-facing display version (`2026.9`, `24.04`) had no way to express either.

Two new configuration surfaces address this.

**`[version_scheme.<id>]`** renders a display label from calendar parts, release ordinals, and declared values:

```toml
[version_scheme.calver]
template = "{{ year }}.{{ month_padded }}.{{ release_of_month }}"

[package.app]
display_version = "calver"
```

**`[package.<id>.values.<id>]`** declares a value, which becomes a template variable and, for file counters, is stamped on every release. Every declaration names exactly one source:

```toml
[package.app.values.build]
file = "build.json" # you create and commit this: {"build": 0}
field = "build"
on_release = "increment" # or { add = { amount = 10 } } or "none"
reset = "version" # iOS release trains; "never" for Play/macOS

[package.app.values.artifact]
hash = "artifacts/app.aab" # sha256 over a file
encoding = "base36" # hex, base32, base36, or digits
length = 8

[package.app.values.run]
env = "GITHUB_RUN_NUMBER"

[package.app.values.rev]
git = "commit_count" # or short_hash

[package.app.values.when]
timestamp = "commit" # or now
```

A declared value reaching a store-facing file uses `value_template` instead of a plain version:

```toml
[[package.app.versioned_files]]
path = "pubspec.yaml"
type = "dart"
value_template = "{{ identity }}+{{ build }}"
```

Template variables now include `identity`, `prerelease`, `year`, `year_short`, `month`, `month_padded`, `quarter`, `day`, `date`, `time`, `release_of_month`, `release_of_quarter`, `release_of_year`, `label`, and every declared value id. `label` is the package's own rendered scheme.

Resolved values and labels are frozen into `ReleaseManifest` and `ReleaseRecord`, so re-rendering a historical release cannot pick up a different timestamp, hash, or counter. The new fields are optional and default to empty, so an existing release record parses unchanged and no migration edge is needed.

## Breaking change

`PreparedRelease`, `ReleaseManifest`, `ReleaseRecord`, `PackageDefinition`, `VersionedFileDefinition`, and `WorkspaceConfiguration` each gain public fields. Any code that constructs these structs with a struct literal must add the new fields:

```rust
ReleaseManifest {
	// ...existing fields...
	values: std::collections::BTreeMap::new(),
	labels: std::collections::BTreeMap::new(),
	label_inputs: monochange_core::versioning::LabelInputs::default(),
	plan: /* ... */,
}
```

Deserialization is unaffected: every new field carries `#[serde(default)]`, so existing JSON artifacts and configuration files continue to load without edits.

## Ordering guarantees

Values fall into three classes. Only counters and ordinals are monotonic; identifiers are not:

- **counters** (`file` with `on_release`) are monotonic within their `reset` policy;
- **ordinals** (`release_of_*`) chain from the previous release record and restart in a new month, quarter, or year;
- **identifiers** (`hash`, `env`, `git`, `timestamp`) carry no ordering guarantee at all.

A hash-derived value is therefore valid in a display label but must not be relied on for ordering. A scheme that uses one is treated as non-monotonic rather than pretending otherwise.

## Counter files

Counter files are yours to create and commit; monochange reads the declared dotted field and rewrites only that value, preserving surrounding formatting and comments. A missing file, a missing field, or a non-integer value is a blocking configuration error naming the path, field, and expected shape — there are no silent zeros:

```text
counter file `build.json` does not exist; create it with its starting value, for example {"build": 0}
```

Adopting monochange in a repository whose app already has a production build number means creating the file once with the current value.

Packages without declared values and without `display_version` behave exactly as before: no counter files are written, no extra fields appear in the manifest, and no state file is created.

## Validation

- a value id may not shadow a context variable name (`year`, `identity`, `label`, …);
- `display_version` must reference a declared scheme, and scheme templates may only use available variables;
- a `value_template` on a package's ecosystem manifest must render a valid `SemVer`, checked by rendering the template and parsing the result.

That last rule permits what Dart and Flutter actually need. A `pubspec.yaml` carries `1.2.3+4`, which is valid `SemVer` build metadata, so a counter appended to the identity is accepted:

```toml
[[package.app.versioned_files]]
path = "pubspec.yaml"
type = "dart"
value_template = "{{ identity }}+{{ build }}"
```

A template that could never parse is still rejected. Calendar versions (`{{ year }}.{{ month }}`), and letter-bearing values such as a `base36` hash in a numeric position, fail with a message naming the offending template.
