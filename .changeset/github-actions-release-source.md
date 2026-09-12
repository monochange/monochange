---
"monochange_core": minor
"monochange_config": minor
"monochange": minor
---

# Release GitHub Actions repositories and tag-versioned packages

Repositories that release by git tag plus provider release — GitHub Actions above all — could not be modeled: every package needed a registry ecosystem, and moving tag aliases had to be maintained by hand.

- `PackageType` gains `github_actions` (aliases `github_actions` and `actions`). The type preset implies `version_source = "tag"`, `tag = true`, `release = true`, publishing disabled, and `initial_version = "0.1.0"`. Discovery accepts a package directory containing `action.yml` or `action.yaml` and syncs a sibling `package.json` version field when present.
- New package and group fields: `version_source` (`manifest` default or `tag`), `initial_version` (baseline when no release tag exists yet), and `floating_tags` (moving tag aliases).
- `PackageType::manifest_file_name` exposes the per-type manifest name and returns `None` for types without a single version-bearing manifest.
- `EffectiveReleaseIdentity`, `ReleaseTarget`, `ReleaseManifestTarget`, and `ReleaseRecordTarget` carry `version_source`, `initial_version`, and `floating_tags` (targets carry `floating_tags` only).
- New helpers `render_floating_tag` and `validate_floating_tag_template_variables` render and validate floating-tag templates with `{{ major }}`, `{{ minor }}`, and `{{ patch }}` variables in addition to the `version_format` variables.
