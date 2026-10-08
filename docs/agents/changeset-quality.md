# Changeset quality

A changeset is a permanent record for its intended readers, who may be product users, developers, or operators. Explain what changed, why it matters to that audience, and how to adapt without reading the source diff.

## Required content

Every changeset body must include:

1. **One H1 headline** (`# Short outcome`) describing what changed for the selected audience, with no trailing period.
2. **An impact summary** explaining why the change matters to those readers.
3. **A focused example when callers must change an invocation, API, configuration, migration, or expected output shape.**

A one-liner that only restates the PR title is not acceptable. The body must give its readers enough detail to act without consulting the source diff.

Do not repeat the headline as the first sentence of the impact summary. Start with the consequence, affected audience, or action instead. monochange selects the final changelog heading depth and package-label placement; authors should not encode either concern in the changeset source.

## Audience-specific streams

Each changeset file targets exactly one configured changelog stream. A type without `stream` uses the built-in `default` stream. A custom type can route the file to a custom stream:

```toml
[changelog.streams.user]
description = "Product release notes"

[changelog.types.app_feature]
bump = "minor"
section = "features"
stream = "user"
```

<!-- {=changesetAudienceRules} -->

Read the stream and type descriptions in `[changelog.streams]` and `[changelog.types]`, then inspect destinations in `[changelog.outputs]` in `monochange.toml` before choosing a type. The type selects the stream. The package id identifies what changed; it does not select the audience. An application can use both developer and product streams.

- Use the default/developer stream for API contracts, deployment, CI, credentials, migrations, and internal maintenance that only developers or operators need to know about. For example, an app's deployment-key validation can use `app: fix` when `fix` belongs to `default`.
- Use a product stream for outcomes people experience while using the app, such as staying signed in after a reload or connecting a repository. For example, `app: website_fix` selects product notes only when that type is configured with `stream = "website"`.
- Write two changesets for the same package when both audiences need an entry. Explain the operational contract in one and the visible outcome in the other. A developer-only change needs no product entry; do not invent a user benefit to fill that stream.

Every target in one changeset file must resolve to the same stream. Within that audience, choose a type whose configured bump matches the release policy. Stream and bump are separate decisions; a native-binary requirement still applies even when the note is developer-facing.

Ensure each intended stream has an output for the package. A package with `changelog = false` has no implicit default output. Check whether a group retains its developer notes; otherwise configure a named developer output. Run `monochange step validate`, preview with `monochange preview --format json`, and inspect each artifact's `output`, `stream`, `owner_id`, and `path`. Render each intended output with `monochange notes --output <id> [--target <id>]`. The preview checks stream consistency; it cannot determine the audience of prose. Review the rendered notes for audience fit.

<!-- {/changesetAudienceRules} -->

For mobile app release policies, a repository may configure `native` as a major/default-stream type and `app_feature` as a minor/user-stream type. Use `native` whenever the diff changes native code or otherwise requires a new store binary; use `app_feature` only when the release is eligible for a patch system such as Shorebird.

## CLI changes

For any change that adds, removes, or modifies a CLI command or flag:

- Show the **exact command invocations** before and after when the invocation itself changed.
- If the command stayed the same and only the result changed, show the command **once** and only show the changed output before/after.
- Do **not** print the same command, config snippet, or other example twice when the example itself did not change.
- Show **config snippets** (`monochange.toml`) when behaviour is driven by configuration.
- Show representative **output** (text or JSON) when the output shape changes.
- Use `# before` / `# after` comments when renaming flags or restructuring commands.

The goal is to highlight the differences, not duplicate unchanged context.

Hypothetical example for a proposed streamlined invocation: the current CLI requires one path per repeated `--changed-paths` flag; the multi-path "After" command below illustrates a future feature rather than current usage.

> # Allow one `monochange step affected-packages --changed-paths` flag to accept several paths

Example body:

> **Before:**
>
> ```bash
> monochange affected --changed-paths src/lib.rs --changed-paths crates/core/src/main.rs
> ```
>
> **After:**
>
> ```bash
> monochange affected --changed-paths src/lib.rs crates/core/src/main.rs
> ```
>
> Repeated `--changed-paths` flags continue to work for compatibility.

When the invocation is unchanged but the output changes, prefer a structure like this:

> # Update `monochange step plan-publish-rate-limits --format json` batch filtering
>
> Command:
>
> ```bash
> monochange step plan-publish-rate-limits --format json
> ```
>
> **Before (output):**
>
> ```json
> { "publishRateLimits": { "batches": ["private", "public"] } }
> ```
>
> **After (output):**
>
> ```json
> { "publishRateLimits": { "batches": ["public"] } }
> ```
>
> Do not repeat the same `monochange step plan-publish-rate-limits --format json` command in both sections.

When a command is **removed**, explain what users should do instead:

> # Remove legacy deployment workflow command
>
> Use your CI platform's native deployment triggers (e.g. a GitHub Actions `workflow_run` event on the release workflow) instead of the legacy deployment wrapper.

## Library API changes

For any change that adds, modifies, or removes a public type, function, or trait in a published crate:

- Show the **type signature or struct definition** before and after.
- Use `// before` / `// after` comments for inline diffs where a full block is too long.
- For renamed items, show the old name struck out and the new name.
- For removed items, show the replacement or migration path.

Example body for a renamed type:

> # Rename `WorkflowDefinition` to `CliCommandDefinition`
>
> **Before (`monochange_config`):**
>
> ```rust
> use monochange_config::WorkflowDefinition;
> let cmd: WorkflowDefinition = config.workflows[0].clone();
> ```
>
> **After:**
>
> ```rust
> use monochange_config::CliCommandDefinition;
> let cmd: CliCommandDefinition = config.cli[0].clone();
> ```

For new APIs, show a minimal but realistic usage example:

> # Add `ChangelogFormat` enum to `monochange_core`
>
> ```rust
> use monochange_core::ChangelogFormat;
>
> let fmt = ChangelogFormat::KeepAChangelog;
> assert_eq!(fmt.to_string(), "keep_a_changelog");
> ```

## Configuration changes

For new or changed `monochange.toml` keys, always show the TOML before and after when the TOML itself changed.

If the config snippet is identical before and after, do not duplicate it. Show the unchanged config once, then show the changed output or behaviour instead.

> **Before (no per-package format override):**
>
> ```toml
> [defaults.changelog]
> path = "{{ path }}/CHANGELOG.md"
> ```
>
> **After:**
>
> ```toml
> [defaults.changelog]
> path = "{{ path }}/CHANGELOG.md"
> format = "keep_a_changelog"
>
> [package.core.changelog]
> format = "monochange" # overrides the default for this package
> ```

## Breaking changes

Any change that requires callers to update their code, config, or workflows must:

- Open the body with a `> **Breaking change**` blockquote.
- List every removed or incompatibly changed item.
- Give a concrete migration path for each.

Example:

> **Breaking change:** `[[workflows]]` config is no longer accepted.
>
> Rename every `[[workflows]]` table to `[cli.<command>]` and move `[[workflows.steps]]` entries to `[[cli.<command>.steps]]`.

### Version migration guides

A version whose release contains breaking changes ships a migration guide, so readers upgrade from one document instead of reconstructing steps from changelog entries:

- File: `docs/src/guide/migrations/<version>.md` (for example `docs/src/guide/migrations/0.11.md`), added to the "Migration guides" part in `docs/src/SUMMARY.md`, newest first.
- Add or update the version guide in the same PR that introduces the breaking change. Never defer it to a later PR.
- Group entries by audience: CLI behaviour first, then configuration and machine-readable schemas, then library APIs.
- Each entry states who is affected, what changed, and the exact update step with before/after examples.
- Reference the guide from the breaking changeset so release notes link the two:

  > **Breaking change:** the default command result is now text.
  >
  > Parsers must request `--format json` or `--format json-min`. See `docs/src/guide/migrations/0.11.md`.

Feature releases without breaking changes and patch releases do not get a guide.

## GUI / app changes

For graphical or browser-based interfaces, embed a screenshot or screen recording link when one is available. If screenshots are not feasible, describe the visual change in enough detail that a user can identify the affected UI element and understand what it looks like now.

Example:

> # Add release summary panel to dashboard
>
> A collapsible **Release summary** card now appears at the top of the project page after a release run completes. It lists each published package, the new version, and a link to the corresponding changelog entry.
>
> ![Release summary panel](docs/screenshots/release-summary-panel.png)

## What counts as too short

Reject or expand a changeset if its body matches any of these patterns:

- A single sentence that only restates the headline.
- "Internal refactor with no user-visible changes" with no evidence.
- A list of file names or function names with no explanation of user impact.
- A PR title copy-pasted verbatim.

The bar is: _could a user who has never seen this repository understand what changed, whether it affects them, and what to do about it?_ If not, the changeset needs more detail.
