---
monochange: minor
monochange_app: patch
monochange_config: minor
monochange_core: patch
monochange_schema: patch
---

# Make versioned file templates field-aware and expose release streams

A `value_template` on a format-mode versioned file ignored its declared `fields` and replaced the first SemVer-looking text in the file, so a derived value such as an artifact digest could land in the wrong field, or the write could fail outright when the file carried no version at all. The template now writes into the explicit `fields` when `format` mode declares them:

```toml
[[package.program.versioned_files]]
path = "deploy/artifact.json"
format = "json"
fields = ["sha256"]
value_template = "{{ artifact_digest }}"
```

Separately, one owner declaring multiple `versioned_files` entries for the same path was silent even though the entries apply in declaration order and a divergent later entry can overwrite an earlier write. `monochange check` and `monochange step validate` now warn once per owner, with exact duplicates and cross-owner file sharing left alone because both are legitimate. Command steps can also gate on what a release rendered: `release.streams` and `release.outputs` list the prepared release's changelog streams and named outputs, so a deployment workflow can express `when = "{{ 'onchain' in release.streams }}"`.
