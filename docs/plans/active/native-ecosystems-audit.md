# Native ecosystem audit

Audit [PR 830](https://github.com/monochange/monochange/pull/830) across discovery, configuration, release preparation, publishing command construction, and registry lookups. Fix demonstrated defects with regression tests; keep registry mutations outside this development task.

- [x] Review the complete diff and trace each new ecosystem through release planning.
- [x] Add failing regressions for real manifest and command failure cases.
- [x] Fix adapter and orchestration defects, and update release notes and docs.
- [ ] Run formatting, lint, full tests, configuration/release preview, affected-package verification, and 100% patch coverage.
- [ ] Commit signed fixes, update the PR, and verify CI and merge readiness.

Confirmed defects and corrections:

- Gradle publish dry runs executed the real release task. Append Gradle's `--dry-run` and test command construction.
- CocoaPods dry runs discarded the selected podspec and validation flags. Preserve both when switching to `pod spec lint`.
- Ruby version updates rewrote commented declarations. Match active assignments at the start of a line.
- JSON podspecs were recognized by filename but parsed as Ruby. Parse their JSON name and version fields.
- Native auto-discovery and init rendering were missing type dispatch arms. Use adapter parsers and the canonical package type spelling.
- Configured native packages dropped ecosystem versioned files, and glob validation treated native files as Go files. Route both paths to their native adapters.
- Gradle rewrites changed separators and line endings, mishandled whitespace-separated Maven coordinates, and ignored custom version keys. Preserve source syntax and validate the selected keys.
- Native publishing used npm's dependency-order settings, and native releases ignored configured lockfile commands. Honor each ecosystem's settings.
- Placeholder pods referenced a hard-coded source repository and invalid source tag. Disable unsupported placeholder publishing.
- Swift typed versioned files were advertised as validation-only but lacked an execution arm. Preserve the manifest without rewriting it.

The regression suite reproduced missing JSON discovery and comment corruption before the fixes. Four CLI integration tests pass, covering auto-discovery, inherited versioned files, Swift package identity, JSON podspec updates, Gradle properties, and execution of a wrapper that refuses to run without `--dry-run`. Native configuration regressions and both final placeholder/Swift dispatch regressions pass. Explicit-path changeset verification covers all ten affected packages. The full test suite, lint checks, and coverage are in progress.

External constraint: the [CocoaPods trunk read-only plan](https://blog.cocoapods.org/CocoaPods-Specs-Repo/) schedules a November 1–7, 2026 trial and a permanent switch on December 2, 2026. Document this service limitation without hard-coding a date-based refusal into the adapter.
