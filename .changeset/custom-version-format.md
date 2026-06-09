---
"monochange": patch
"monochange_config": patch
"monochange_core": patch
"monochange_schema": patch
"@monochange/skill": patch
---

# Add custom `version_format` tag templates for package and group release identities

`primary` and `namespaced` continue to work as presets, while custom formats such as `{{ ecosystem }}/{{ name }}/v{{ version }}` can use `{{ name }}`, `{{ version }}`, and `{{ ecosystem }}`. Custom formats must include `{{ version }}`, render valid Git tag names, and avoid collisions with other release owners.
