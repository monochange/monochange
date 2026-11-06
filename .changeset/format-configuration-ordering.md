---
monochange: minor
monochange_config: minor
monochange_app: patch
"@monochange/skill": patch
---

# Add canonical `monochange format` command

Adds `monochange format` to rewrite `monochange.toml` in the idiomatic layout: top-level sections in a fixed order, `[package.*]` and `[group.*]` tables sorted by id, group members sorted alphabetically, and `[cli.*]` workflows last. Comments stay attached to their tables and the document header stays at the top.

The command formats the result with taplo using the repository's tab-based, 100-column configuration, so it does not require a separate taplo installation. Use `monochange format --check` to fail CI when the file needs formatting without writing changes. The repository's `lint:monochange` task and CI lint job now run that check.
