---
monochange_schema: major
---

# Publish the release-request body keys in the configuration schema

`[source.pull_requests]` gained two keys, so the published `monochange.toml` schema carries them:

```toml
[source.pull_requests]
# "full" inlines the release notes; "summary" renders the header, target list,
# and changelog paths only.
body_style = "full"
# Optional cap for the rendered request body. Defaults to the provider's limit.
max_body_chars = 65536
```

Release records are unchanged, because the keys affect only how the release pull request body is rendered. `monochange.schema.json` advances to v0.8 alongside the v0.8 change that removes `[changelog.style].package_label_placement`; both land in the same release, and the v0.7 to v0.8 migration edge stays a no-op for record payloads.
