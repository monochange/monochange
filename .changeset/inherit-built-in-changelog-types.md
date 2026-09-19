---
"monochange_config": patch
---

# Inherit built-in changelog types under configured sections and types

Declaring any `[changelog.sections]` or `[changelog.types]` entry replaced monochange's built-in section and type set outright instead of extending it. A repository that customized a single heading silently lost `minor`, `patch`, and every stream type it did not restate:

```toml
[changelog.types.app_feature]
bump = "minor"
section = "app_features"
```

With that table, the built-in `fix` type stopped resolving, so a changeset written as `core: fix` failed with a message listing only `app_feature`:

```text
config error: failed to parse .changeset/change.md: target `core` has invalid scalar change type `fix`; valid types: app_feature
```

The message made `fix` look like something monochange had never supported, rather than a key the merge had dropped. Configuring sections alone was worse: a repository that declared only `[changelog.sections]` had no types at all and every changeset failed with `no configured types are available for this target`.

## After

`[changelog.sections]` and `[changelog.types]` add to the built-in vocabulary. A declared key overrides the built-in entry of the same name and every other built-in key stays available:

```toml
[changelog.types.app_feature]
bump = "minor"
section = "app_features"
```

`core: fix` still resolves under that table, the CLI and interactive prompts offer the merged set, and a type may reference either a declared section or a built-in one. Per-package and per-group `excluded_changelog_types` remain the way to narrow the vocabulary for a target, including for inherited types.

`[changelog.templates]` is unchanged: it stays an ordered preference list, so declaring templates replaces the built-in list rather than appending to it.
