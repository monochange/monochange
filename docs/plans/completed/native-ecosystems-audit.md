# Native ecosystem audit

Audit [PR 830](https://github.com/monochange/monochange/pull/830) across discovery, configuration, release preparation, publishing command construction, and registry lookups. Fix demonstrated defects with regression tests; keep registry mutations outside this development task.

- [x] Review the complete diff and trace each new ecosystem through release planning.
- [x] Add failing regressions for real manifest and command failure cases.
- [x] Fix adapter and orchestration defects, and update release notes and docs.
- [x] Run formatting, lint, full tests, configuration/release preview, affected-package verification, and 100% patch coverage.
- [x] Prepare verified signed fixes and the PR review record. CI and merge-queue follow-up are tracked on the linked PR.

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

The regression suite reproduced missing JSON discovery and comment corruption before the fixes. Four CLI integration tests pass, covering auto-discovery, inherited versioned files, Swift package identity, JSON podspec updates, Gradle properties, and execution of a wrapper that refuses to run without `--dry-run`.

Validation evidence:

- `fix:all`, `build:all`, and `lint:all` pass.
- The full Rust nextest run passes all 4,279 tests without exclusions; Rust doc tests and all 145 Node tests pass. Both additional Swift/no-op and native-glob regressions pass, along with all five Gradle publishing regressions.
- Explicit-path and comparison-based `affected-packages --from origin/main --verify` both cover all ten changed packages and pass changeset policy.
- Workspace validation and `prepare-release --dry-run --format json` pass. The preview plans main group 0.21.0 and records the expected output/stream pairs, including the developer-facing website notes.
- Fresh workspace coverage passes the 70% floor with 97.33% line coverage. The final patch check covers all 922/922 executable changed lines (100%). The final instrumented run hit seven npm semantic-analyzer timeouts; rerunning that crate and the remaining coverage crates with two test threads passes, including all 127 npm tests. No assertions, time limits, or snapshots were changed for those timeouts.

`test:all` reaches its agent-free skill contracts after the passing Rust/doc/Node suites, then fails six assertions against old CLI exit codes and text: `run-change-errors-without-cli-config`, `cli-change-wrapper-honors-dry-run`, `discover-md-renders-a-markdown-report`, `unknown-format-is-rejected`, `run-without-cli-config-errors`, and `dry-run-prints-and-does-not-deploy`. The contract files and CLI output/argument definitions are unchanged against `origin/main`. Keep this limitation explicit rather than changing unrelated behavior or weakening the assertions in this ecosystem PR.

External constraint: the [CocoaPods trunk read-only plan](https://blog.cocoapods.org/CocoaPods-Specs-Repo/) schedules a November 1–7, 2026 trial and a permanent switch on December 2, 2026. Document this service limitation without hard-coding a date-based refusal into the adapter.
