---
monochange_config: patch
---

# Validate release titles against their rendered context

Validate package, group, and default `release_title` and `changelog_version_title` templates against the actual release context. Both title fields accept `id`, `version`, `previous_version`, `date`, `time`, `datetime`, `changes_count`, `tag_url`, and `compare_url`, including Jinja filters and conditionals. Invalid syntax and version-value variables such as `name`, `year`, and declared counters now produce a configuration error instead of rendering empty title text or falling back to a version.
