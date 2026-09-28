---
"monochange": fix
"monochange_npm": fix
"monochange_dart": fix
---

# Rewrite quoted dependency keys when updating lockfiles and manifests

pnpm quotes scoped package names in `pnpm-lock.yaml` (`'@acme/api':`), and YAML permits quoted keys in a `pubspec.yaml` dependency section. `monochange prepare` reported those lockfiles as changed but wrote the file back unchanged, leaving the lockfile pinned to the old version while the manifest moved. Any npm or pnpm workspace with a scoped internal dependency, and any Dart workspace that quotes a dependency key, was affected.

The cause was line parsing that split on the first `:` without stripping YAML quoting, so the lookup key (`'@acme/api'`) never matched the bare package name in the version map. Quoted keys are now parsed as YAML scalars, which also handles the doubled-quote escapes (`''` inside single quotes, `\"` inside double quotes). Only the lookup key is unquoted: the original quoting is preserved byte for byte when the line is rewritten, because pnpm regenerates and compares these files.

**Before (lockfile contents after `monochange prepare` with a `minor` changeset for `@acme/api`):**

```yaml
importers:
  .:
    dependencies:
      "@acme/api": 2.3.1
```

**After:**

```yaml
importers:
  .:
    dependencies:
      "@acme/api": 2.4.0
```

The same unquoting fixes the `monochange_dart` dependency-sorted lint rule, which compares source key order against the parsed mapping. A sorted section that quoted its keys was reported as unsorted forever.

`link:` and `workspace:` references are still skipped, including quoted and double-quoted forms.
