<!-- {@changesetPhilosophy} -->

**A changeset records what changed for its intended audience.**

Choose the audience stream first, then describe the relevant behavior for the artifact type:

<!-- {/changesetPhilosophy} -->

<!-- {@changesetAudienceRules} -->

Read the stream and type descriptions in `[changelog.streams]` and `[changelog.types]`, then inspect destinations in `[changelog.outputs]` in `monochange.toml` before choosing a type. The type selects the stream. The package id identifies what changed; it does not select the audience. An application can use both developer and product streams.

- Use the default/developer stream for API contracts, deployment, CI, credentials, migrations, and internal maintenance that only developers or operators need to know about. For example, an app's deployment-key validation can use `app: fix` when `fix` belongs to `default`.
- Use a product stream for outcomes people experience while using the app, such as staying signed in after a reload or connecting a repository. For example, `app: website_fix` selects product notes only when that type is configured with `stream = "website"`.
- Write two changesets for the same package when both audiences need an entry. Explain the operational contract in one and the visible outcome in the other. A developer-only change needs no product entry; do not invent a user benefit to fill that stream.

Every target in one changeset file must resolve to the same stream. Within that audience, choose a type whose configured bump matches the release policy. Stream and bump are separate decisions; a native-binary requirement still applies even when the note is developer-facing.

Ensure each intended stream has an output for the package. A package with `changelog = false` has no implicit default output. Check whether a group retains its developer notes; otherwise configure a named developer output. Run `monochange step validate`, preview with `monochange preview --format json`, and inspect each artifact's `output`, `stream`, `owner_id`, and `path`. Render each intended output with `monochange notes --output <id> [--target <id>]`. The preview checks stream consistency; it cannot determine the audience of prose. Review the rendered notes for audience fit.

<!-- {/changesetAudienceRules} -->

<!-- {@changesetLifecycleRules} -->

As features are added and removed, changesets must be actively managed throughout the development lifecycle:

1. **Analyze existing changesets** before creating new ones: read every `.changeset/*.md` file and understand what each covers
2. **Determine the appropriate action** for each change:
   - **Create new**: For genuinely new changes (preferred)
   - **Update existing**: When expanding the scope of a change already described
   - **Remove obsolete**: When the feature was reverted or the change no longer exists
   - **Replace**: When the same intent is now implemented differently

**Golden rule:** Err on the side of creating a new changeset. It's easier to consolidate later than to split apart.

**New package rule:** When a PR introduces a new published package or crate, the first changeset for that package must use a `major` bump for the new package entry.

<!-- {/changesetLifecycleRules} -->

<!-- {@changesetLifecycleDecisionMatrix} -->

| Scenario                          | Action                   | Rationale                                       |
| --------------------------------- | ------------------------ | ----------------------------------------------- |
| New feature added                 | **Create new**           | Granular tracking of distinct changes           |
| New published package or crate    | **Create new**           | First release note should use a `major` bump    |
| Existing feature expanded         | **Update existing**      | Keep related changes together                   |
| Feature removed or reverted       | **Remove changeset**     | Don't release notes for removed features        |
| Same change, different approach   | **Replace changeset**    | Document the actual implementation              |
| Multiple small related changes    | **Create new** (grouped) | Summarize when exceeding threshold              |
| Bug found in unreleased feature   | **Update existing**      | Combine fix with feature, not a separate entry  |
| Refactor of unreleased change     | **Update existing**      | Rewrite description to reflect new structure    |
| Changeset references removed code | **Remove changeset**     | Stale changesets create confusing release notes |

<!-- {/changesetLifecycleDecisionMatrix} -->

<!-- {@changesetGranularityRules} -->

When deciding how many changesets to create for a single PR or branch:

| Change type                    | Library         | Application                 | CLI / LSP / MCP |
| ------------------------------ | --------------- | --------------------------- | --------------- |
| Single new feature             | Separate        | Separate                    | Separate        |
| Multiple related API additions | 3+ → group      | 2+ → group                  | 2+ → group      |
| Internal refactoring only      | Patch           | Patch                       | Patch           |
| Breaking + non-breaking mixed  | Separate        | Separate                    | Separate        |
| New routes/pages               | N/A             | 2+ → summarize              | N/A             |
| New commands/tools             | N/A             | N/A                         | 2+ → summarize  |
| **Documentation-only**         | 10+ → summarize | 10+ → summarize             | 10+ → summarize |
| **UX / visual changes**        | N/A             | Separate (with screenshots) | N/A             |

**Summarize** = Create a single changeset with a grouped description. **Separate** = Create individual changesets (or mark as breaking).

<!-- {/changesetGranularityRules} -->
