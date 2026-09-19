---
monochange:
  bump: patch
  type: docs
"@monochange/skill":
  bump: patch
  type: docs
---

# Correct invalid configuration examples and tighten documentation prose

Several documented configuration examples did not parse, and the skill's migration guidance showed before-and-after commands that were identical to each other.

Examples that were wrong and are now corrected:

- `[ecosystems.*].lockfile_commands` was documented as a list of bare strings in the skill's `configuration.md`, `examples/quickstart.md`, and `examples/migration.md`. Every entry is a table with a `command` field, so the documented form failed with `invalid type: string, expected struct LockfileCommandDefinition`. The skill examples now use the table form.
- The GitHub automation example ended with an orphaned `name`, `trigger`, `release_targets`, and `requires` fragment after `[changesets.classification]`. None of those keys exist in `monochange.toml`, and the fragment silently parsed as nothing because it sat under the wrong table. It is removed from the shared template and from every generated copy.
- The changelog section threshold field was documented as `collapsed`; the real key is `collapse`, and `ignored` must be at least as large as it.
- The changelog style guide listed `heading`, `rule`, `inline`, and `plain` as `section_separator`, `package_label_placement`, and `package_label_style` values. The real values are `blank_line`, `thematic_break`, `none`, `after_heading`, `after_change`, `badge`, and `omit`.
- The publishing example set `trusted_publishing = true` and then opened `[ecosystems.npm.publish.trusted_publishing]` in the same document, which is a duplicate key. `trusted_publishing` is a boolean or a table, so the examples now show each form separately.
- The publish workflow example bound `format`, `mode`, `package`, `ci`, `group`, and `ecosystem` inputs that the command never declared, which failed validation with `inherits input ... but the command does not declare it`.
- The release PR example used `OpenReleaseRequest` without configuring `[source]`, which validation rejects.

Prose changes: prose em dashes are gone from the book, the skill, and the repository readmes; `commands.md` now shows real command-path migrations instead of no-op examples; and the configuration reference gained annotated examples for package fields, version formats, floating tags, versioned files, changelog style, group filters, and publish policy.

`[ecosystems.*].enabled`, `roots`, and `exclude` are still parsed without filtering discovery, and `[defaults].include_private` still does not filter what `step discover` reports. Those notes are now stated as observed behavior rather than left ambiguous.
