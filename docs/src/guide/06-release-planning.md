# Release planning

Create a changeset with the CLI:

<!-- {=releaseChangesAddCommand} -->

```bash
monochange run change --package sdk-core --bump minor --reason "public API addition"
monochange run change --package sdk-core --bump patch --type security --reason "rotate signing keys" --details "Roll the signing key before the release window closes."
monochange run change --package sdk-core --bump none --type docs --reason "clarify migration guidance" --output .changeset/sdk-core-docs.md
monochange run change --package sdk-core --bump major --version 2.0.0 --reason "break the public API" --output .changeset/sdk-core-major.md
```

Or use interactive mode to select packages, bumps, and options from a guided wizard:

```bash
monochange run change -i
```

Interactive mode automatically prevents conflicting selections (a group and one of its members) and lets you pick per-package bumps and optional explicit versions.

<!-- {/releaseChangesAddCommand} -->

Or write one manually with configured package or group ids:

<!-- {=releaseManualChangesetExample} -->

```markdown
---
sdk-core:
  bump: patch
  type: security
---

# rotate signing keys

Roll the signing key before the release window closes.
```

<!-- {/releaseManualChangesetExample} -->

Group-targeted changesets are also valid:

```markdown
---
sdk: minor
---

# coordinated SDK release
```

## Package ids first, group ids when the release boundary is shared

Use a **package id** when one package changed directly:

```markdown
---
sdk-core: minor
---

# add changelog rendering API
```

Use a **group id** when the outward release note should be owned by the whole group:

```markdown
---
sdk: minor
---

# coordinated SDK release
```

A quick rule of thumb:

- **package id**: the leaf package changed and monochange can propagate the rest
- **group id**: the note should read as one coordinated release for all members

<!-- {=releaseExplicitVersionChangesetExample} -->

Prefer the inline form: write a configured change type as the target value when its default bump is what you want (`sdk-core: docs` for a documentation-only change, `sdk-core: fix` for a patch fix). Use scalar bumps (`sdk-core: minor`) for plain bumps without a custom type. Use the object syntax only when you need to pin an exact version, combine `bump`, `version`, and `type`, or override a type's default bump (for example `docs` with a `patch` bump):

```markdown
---
sdk-core:
  bump: major
  version: "2.0.0"
---

# promote to stable
```

When `version` is provided without `bump`, the bump is inferred from the current version. If the package belongs to a version group, the explicit version propagates to the whole group. Overriding a type's default bump is possible but should be avoided unless the user explicitly asks for that version behavior.

<!-- {/releaseExplicitVersionChangesetExample} -->

When a dependent package changes only because another package moved first, author that context explicitly with `caused_by`:

```bash
monochange run change --package sdk-config --bump none --caused-by sdk-core --reason "dependency-only follow-up"
```

```markdown
---
sdk-config:
  bump: patch
  caused_by: ["sdk-core"]
---

# update dependency on sdk-core
```

And when the package is affected but does not deserve a consumer-facing version bump, use `bump: none`:

```markdown
---
sdk-config:
  bump: none
  caused_by: ["sdk-core"]
  type: docs
---

# document the coordinated release
```

If multiple changesets specify conflicting explicit versions for the same package or group, monochange uses the highest semver version and emits a warning by default. Set `defaults.strict_version_conflicts = true` to fail instead.

monochange keeps its own changeset standard rather than reusing a narrower external parser. Top-level frontmatter keys are package ids or group ids only. Each target can use scalar shorthand or the object syntax with `bump`, `version`, and `type`, while the markdown body is split into a summary plus optional detailed follow-up paragraphs. Authored heading depth is normalized when release notes are rendered, so use natural markdown headings in the changeset body instead of hard-coding output depth.

Validate before planning:

```bash
monochange step validate
```

## Semantic SemVer guardrails

Release planning also runs semantic analysis for the detected git change frame when it can. The analyzer output is advisory release evidence, not a replacement for changesets:

- removed or modified public API/export evidence can raise the minimum bump to `major`;
- added public API/export evidence can raise the minimum bump to `minor`;
- dependency and metadata evidence is treated as `patch` advisory context;
- analyzer failures become release-plan warnings instead of blocking the command.

Dry-run JSON and release manifest payloads include this data under `compatibilityEvidence`. Human reviewers should compare that evidence with the authored changesets before preparing a release.

## Release manifests vs release records

Release planning and release repair use two different artifacts on purpose.

- the cached release manifest at `.monochange/release-manifest.json` captures **what monochange is preparing right now** during command execution.
- `ReleaseRecord` captures **what a release commit historically declared** inside the monochange-managed release commit body.

Use the manifest when you want execution-time automation such as CI artifacts, MCP/server responses, previews, or downstream machine-readable release data.

Use the release record when you want to rediscover or repair a release later from a tag or descendant commit.

That is why `ReleaseRecord` does not replace the cached release manifest: one is an execution-time automation artifact, the other is a durable git-history artifact.

When you need to inspect or repair a recent release, see [Repairable releases](./12-repairable-releases.md).

Generate a plan directly when you want to inspect the raw planner output:

<!-- {=projectPlanCommand} -->

```bash
monochange run release --dry-run --format json
```

<!-- {/projectPlanCommand} -->

For human-readable local output, `monochange run release --dry-run` defaults to terminal-friendly markdown. Use `--format text` when you want the older plain-text style, or keep `--format json` for automation.

Preferred repository command flow:

<!-- {=projectDryRunCommand} -->

```bash
monochange run release --dry-run --format json
```

<!-- {/projectDryRunCommand} -->

When you want a reviewable patch-style preview of the filesystem changes without mutating the workspace, add `--diff`:

```bash
monochange run release --dry-run --diff
monochange run release --dry-run --format json --diff
```

Markdown and text output render unified diffs directly in the terminal. JSON output wraps the normal manifest payload under `manifest` and adds `fileDiffs` entries for each changed file.

A good planning loop looks like this:

```bash
monochange step validate
monochange step discover --format json
monochange step diagnose-changesets --format json
monochange run release --dry-run --diff
```

Use each command for a different question:

- `monochange step validate`: is the config and changeset set valid?
- `monochange step discover --format json`: which package ids, groups, and dependency edges exist?
- `monochange step diagnose-changesets --format json`: who introduced these changesets and what review context is attached?
- `monochange run release --dry-run --diff`: what exact files would change if I prepared the release now?

### Compare preview modes

Use the preview mode that matches the decision you are trying to make:

| Command                                                 | Best for                                      |
| ------------------------------------------------------- | --------------------------------------------- |
| `monochange run release --dry-run`                      | Human review in the terminal                  |
| `monochange run release --dry-run --diff`               | Human review plus exact file patches          |
| `monochange run release --dry-run --format json`        | Automation, scripts, MCP clients              |
| `monochange run release --dry-run --format json --diff` | Automation that also needs file patch details |

When you want command semantics without any command-line noise, add `--quiet`. Quiet mode suppresses stdout/stderr and uses dry-run behavior for release-oriented commands so the workspace stays unchanged.

<!-- {=projectReleaseCommand} -->

```bash
monochange run release
```

<!-- {/projectReleaseCommand} -->

<!-- {=releaseWorkflowBehavior} -->

`monochange run release` is a config-driven workflow command only when your repository defines a `[cli.release]` table. `monochange init` writes a minimal starter config and does not seed default workflow aliases, so use the immutable `monochange step prepare-release` command unless you add your own named workflow.

The binary no longer ships a hidden default workflow set for commands such as `discover`, `change`, `release`, `affected`, `diagnostics`, `repair-release`, `publish`, or `publish-plan`. Those names exist under `monochange run <name>` only when your config defines them. If a repository has not opted into a named workflow, use the immutable step command instead, for example `monochange step discover`, `monochange step create-change-file`, `monochange step prepare-release`, `monochange step affected-packages`, `monochange step diagnose-changesets`, `monochange step retarget-release`, `monochange step publish-readiness`, or `monochange step plan-publish-rate-limits`.

`monochange step validate` is the immutable built-in step command for normal preflight checks. Do not define `[cli.validate]` or `[cli.step]` in `monochange.toml`; those names are reserved for built-in commands.

Configured workflows like `monochange run commit-release` combine `PrepareRelease` with later stateful steps such as `CommitRelease`. Provider request workflows such as `monochange run release-pr` can add `OpenReleaseRequest`. Keep both as explicit `[cli.*]` workflow commands when you want a durable, named release process.

Current `PrepareRelease` behavior:

- reads `.changeset/*.md`
- computes one synchronized release plan from discovered change files
- updates native manifests plus configured changelogs and versioned files
- renders changelog files through structured release notes using the configured `monochange` or `keep_a_changelog` format
- groups release notes into default `Breaking changes`, `Features`, `Fixes`, and `Notes` sections, with package/group overrides available through `extra_changelog_sections`
- applies workspace-wide release-note templates from `[release_notes].change_templates`
- refreshes the cached `.monochange/release-manifest.json` artifact during `PrepareRelease` for downstream automation
- can preview or publish provider releases via `PublishRelease`
- can preview or open/update release requests via `OpenReleaseRequest`
- can comment on released issues via `CommentReleasedIssues`
- can evaluate pull-request changeset policy via `AffectedPackages` using changed paths and labels supplied by CI
- applies group-owned release identity for outward `tag`, `release`, and `version_format`
- deletes consumed change files only after a successful non-dry-run execution
- leaves the workspace untouched during `--dry-run` except for explicitly requested outputs such as a rendered release manifest or release preview

A GitHub Actions check can pass changed paths and labels directly into a policy workflow, for example:

<!-- {/releaseWorkflowBehavior} -->

<!-- {=changesetPolicyGitHubActionWorkflow} -->

```yaml
name: changeset-policy

on:
  pull_request:
    types:
      - opened
      - synchronize
      - reopened
      - labeled
      - unlabeled

concurrency:
  group: changeset-policy-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true

jobs:
  check:
    timeout-minutes: 60
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: read
    steps:
      - name: checkout repository
        uses: actions/checkout@v6

      - name: setup
        uses: ./.github/actions/devenv
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - name: collect changed files
        id: changed
        uses: tj-actions/changed-files@v46

      - name: run changeset policy
        env:
          PR_LABELS_JSON: ${{ toJson(github.event.pull_request.labels.*.name) }}
          CHANGED_FILES: ${{ steps.changed.outputs.all_changed_files }}
        shell: bash
        run: |
          set -euo pipefail

          mapfile -t labels < <(jq -r '.[]' <<<"$PR_LABELS_JSON")
          args=(step affected-packages --format json --verify)

          for path in $CHANGED_FILES; do
            args+=(--changed-paths "$path")
          done

          for label in "${labels[@]}"; do
            args+=(--label "$label")
          done

          devenv shell -- monochange "${args[@]}" | tee policy.raw
          awk 'BEGIN { capture = 0 } /^\{/ { capture = 1 } capture { print }' policy.raw > policy.json
          jq -e '.status != "failed"' policy.json >/dev/null
```

<!-- {/changesetPolicyGitHubActionWorkflow} -->

Planning rules:

<!-- {=releasePlanningRules} -->

- `monochange run change` defaults `--bump` to `patch`; use `--bump none` when you want a type-only or version-only entry, and pass `--version` to pin an explicit release version
- markdown change files use package/group ids as the only top-level frontmatter keys, with scalar shorthand for `none`/`patch`/`minor`/`major` or configured change types, plus object syntax for `bump`, `version`, `type`, and `caused_by`
- when `version` is given without `bump`, the bump is inferred by comparing the current and target versions
- explicit versions from grouped members propagate to the group version; conflicts take the highest semver or fail when `defaults.strict_version_conflicts = true`
- prefer package ids over group ids in authored changesets when possible; direct package changes still propagate to dependents and synchronize configured groups
- optional change `type` values can route entries into custom changelog sections, and configured section `default_bump` values let scalar type shorthand imply the desired semver behavior
- `caused_by` references package or group ids and suppresses only the matching dependency-propagation records; use object syntax whenever you need it
- `monochange run change` accepts repeated `--caused-by <id>` flags, and `--bump none` is the right fit when you want to acknowledge an affected package without forcing a user-facing version bump
- `monochange run change` can write to a deterministic path with `--output ...`
- change templates support detailed multi-line release-note entries through `{{ details }}`, compact metadata blocks through `{{ context }}`, and fine-grained linked metadata like `{{ change_owner_link }}`, `{{ review_request_link }}`, and `{{ closed_issue_links }}`
- dependents resolve their propagation policy most-specific-first: `[[package]].bump_propagation` overrides `group` declarations, which override `[defaults].bump_propagation`, which falls back to the `[defaults].parent_bump` floor; `inherit` matches the source's own severity (optionally clamped by `bump_propagation_max`) and fixed severities act as floors
- computed compatibility evidence can still escalate both the changed crate and its dependents when provider analysis produces it
- configured groups synchronize before final output is rendered
- release targets carry effective `tag`, `release`, and `version_format` metadata
- release-manifest JSON captures release targets, changelog payloads, authored changesets, linked changeset context metadata, changed files, and the synchronized release plan for downstream automation
- `PublishRelease` reuses the same structured release data to build provider release requests for grouped and package-owned releases
- `OpenReleaseRequest` reuses the same structured release data to render release-request summaries, branch names, and idempotent provider updates
- `CommentReleasedIssues` can use linked changeset context metadata to add follow-up comments to closed issues after a release is published
- `AffectedPackages` evaluates changed paths, skip labels, and changed `.changeset/*.md` files into reusable pass/skip/fail diagnostics and optional failure comments
- CLI text and JSON output render workspace paths relative to the repository root for stable snapshots and automation

<!-- {/releasePlanningRules} -->

### Diagnostics vs. release records

These commands answer different questions:

- `monochange step diagnose-changesets --format json`: what is currently pending in `.changeset/*.md`, and who introduced it?
- `monochange step release-record --from <ref>`: what did a past release commit declare durably in git history?
- `monochange step tag-release --from HEAD`: if `HEAD` is the merged release commit, which release tags should be created now?

Use diagnostics **before** you release. Use release records **after** a release exists and you need to inspect it. Use `monochange step tag-release` in post-merge CI when the release commit has landed on the default branch and you want to create the declared tag set from that durable history record.

Across release-oriented commands, global `--quiet` suppresses stdout/stderr and reuses dry-run behavior for commands that support it.

## Concurrency

`monochange run release` is designed for sequential execution. Do not run multiple `monochange run release` commands concurrently on the same workspace. There is no file locking, so concurrent runs could produce duplicate changelog entries, inconsistent version files, or corrupted release records. If you need parallel release preparation across workspaces, use separate working copies.
