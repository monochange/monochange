# Changeset guide

## Before writing

- Inspect the diff.
- Read existing `.changeset/*.md` files.
- Inspect `monochange.toml` for package ids and groups, stream and type descriptions, and output destinations.
- Decide whether this is create, update, merge, split, or delete.

## Targeting

Prefer package ids. Use group ids only for group-owned releases. If a dependent package is included only because another package changed, use `caused_by` and consider `bump: none`.

## Audience selection

<!-- {=changesetAudienceRules} -->

Read the stream and type descriptions in `[changelog.streams]` and `[changelog.types]`, then inspect destinations in `[changelog.outputs]` in `monochange.toml` before choosing a type. The type selects the stream. The package id identifies what changed; it does not select the audience. An application can use both developer and product streams.

- Use the default/developer stream for API contracts, deployment, CI, credentials, migrations, and internal maintenance that only developers or operators need to know about. For example, an app's deployment-key validation can use `app: fix` when `fix` belongs to `default`.
- Use a product stream for outcomes people experience while using the app, such as staying signed in after a reload or connecting a repository. For example, `app: website_fix` selects product notes only when that type is configured with `stream = "website"`.
- Write two changesets for the same package when both audiences need an entry. Explain the operational contract in one and the visible outcome in the other. A developer-only change needs no product entry; do not invent a user benefit to fill that stream.

Every target in one changeset file must resolve to the same stream. Within that audience, choose a type whose configured bump matches the release policy. Stream and bump are separate decisions; a native-binary requirement still applies even when the note is developer-facing.

Ensure each intended stream has an output for the package. A package with `changelog = false` has no implicit default output. Check whether a group retains its developer notes; otherwise configure a named developer output. Run `monochange step validate`, preview with `monochange preview --format json`, and inspect each artifact's `output`, `stream`, `owner_id`, and `path`. Render each intended output with `monochange notes --output <id> [--target <id>]`. The preview checks stream consistency; it cannot determine the audience of prose. Review the rendered notes for audience fit.

<!-- {/changesetAudienceRules} -->

## Structure

Simple bump syntax:

```md
---
"package-id": patch
---

# Short summary for the intended audience

Explain the behavior change and impact.
```

Object syntax with a configured changelog type:

```md
---
"package-id":
  bump: patch
  type: fix
---

# Short summary for the intended audience

Explain the behavior change and impact.
```

## Review checklist

- Type selects the intended audience stream and its bump matches release policy.
- Heading is sentence case and no trailing full stop if project policy requires it.
- Breaking changes have migration instructions.
- Similar package notes in the same stream are combined instead of duplicated.
- `monochange step validate` passes.
- A release preview passes and each rendered output contains only notes for its audience.
