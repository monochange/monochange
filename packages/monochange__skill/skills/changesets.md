# Changesets

Changesets are explicit release intent. They tell monochange which package or group should be considered for a version bump and what human-facing release note to render.

A good changeset answers three questions: what public behavior changed, who is affected, and how monochange should version the affected release target. It should not be a raw commit log or a list of touched files.

## CLI creation

Prefer the repository's configured workflow if present:

```bash
monochange run change --package @acme/api --bump minor --type feat --reason "Add webhook delivery filters"
```

If `monochange run change` is not configured, use the step command directly:

```bash
monochange step create-change-file --package @acme/api --bump minor --reason "Add webhook delivery filters"
```

Always run `monochange step validate` after creating or editing changesets.

The configured command may expose more inputs than the portable step command, such as `--type`, `--caused-by`, or repository-specific defaults. Check `monochange help change` or the `[cli.change]` table before relying on a flag name.

## File shape

The frontmatter keys must be configured package ids or group ids. Quote ids that contain `@`, `/`, dots, or other punctuation so the YAML/TOML-like frontmatter is unambiguous.

Simple package-to-bump syntax:

```md
---
"@acme/api": minor
---

# Add webhook delivery filters

Users can now filter webhook deliveries by event type and delivery status.
```

When using configured changelog types, the type can be the target value when it maps to the desired default bump:

```md
---
"@acme/api": feat
---

# Add webhook delivery filters

Users can now filter webhook deliveries by event type and delivery status.
```

Prefer the inline form. Whenever the bump you want matches the configured type's default bump, write the type as the target value and nothing else. For example, `docs` maps to `bump: none` in a typical config, so a documentation-only change is:

```md
---
"@acme/api": docs
---

# Document the webhook behavior

Explain the retry and delivery semantics in the README.
```

This applies to every configured type: `feat` implies its default bump, `fix` implies its default, `docs` implies `none`, and so on. Use the inline form as the default choice. Object syntax is reserved for cases where the inline form cannot express the intent.

Use object syntax when you need `bump`, `type`, `version`, or `caused_by` together:

```md
---
"@acme/api":
  bump: minor
  type: feat
---

# Add webhook delivery filters

Users can now filter webhook deliveries by event type and delivery status.
```

Multiple targets are allowed when one user-facing change spans packages:

```md
---
"@acme/api":
  bump: patch
  type: fix
"@acme/ui":
  bump: patch
  type: fix
---

# Preserve dashboard filters after retrying requests

Both the API response and the UI retry flow now keep the same filter state.
```

## Audience streams

Every changeset file resolves to exactly one changelog stream. Types without an explicit stream use the built-in `default` stream. Repositories can route types such as `app_feature` to a custom `user` stream in `monochange.toml`.

All targets in one file must resolve to the same stream. When the same implementation needs both developer and user notes, write two changesets with audience-appropriate wording:

```md
---
core: fix
---

# Preserve decoded preview state in `PreviewController.retry`

The controller now reuses the cached decode result when the request retry path rebuilds its provider state.
```

```md
---
app: app_feature
---

# Make preview retries faster

Previews now recover without repeating completed work, so users return to their artwork sooner after a network interruption.
```

The first file belongs in the default/developer stream and names the affected implementation contract. The second belongs in the user stream and describes only the visible outcome. Do not combine the audiences in one body or duplicate the same prose across both files.

Run `monochange step validate` after authoring. It rejects a file whose target types cross streams. Then run `monochange step prepare-release --dry-run --format json` and inspect each changelog object's `output`, `stream`, `owner_id`, and `path`; those fields are the audit trail for where each note will be published.

Use explicit versions only when you need a specific version rather than semver bump calculation:

```md
---
"@acme/api":
  bump: minor
  type: feat
  version: "2.5.0"
---

# Stabilize webhook filter endpoints
```

### Overriding a type's default bump

Object syntax also lets you keep a type while changing its bump. For example, `docs` defaults to `bump: none`, but you might want a docs change to ship a `patch` bump, or a `minor` bump:

```md
---
"@acme/web":
  bump: patch
  type: docs
---

# Document the webhook behavior
```

Overrides like this are possible, but treat them as the exception. Only override a type's default bump when the user explicitly asks for that version behavior; otherwise use the inline form and let the type's default bump stand.

## `caused_by`

Use `caused_by` when a package is affected because another package changed.

```md
---
"@acme/ui":
  bump: none
  type: none
  caused_by: ["@acme/api"]
---

# Rebuild UI package for API dependency metadata

No user-facing UI behavior changed.
```

`caused_by` can reference package ids or group ids. In CLI form, pass repeated `--caused-by <id>` flags when the configured workflow exposes that input.

Use `caused_by` to explain propagation instead of pretending a dependent package has its own feature or fix. This keeps changelogs honest while still preserving enough metadata for release planning and policy checks.

## Bump rules

- `major`: breaking API, CLI, protocol, data, or user workflow changes.
- `minor`: new user-facing functionality or behavior.
- `patch`: fixes and compatible improvements.
- `none`: documentation, tests, rebuilds, or dependency/context notes with no version impact.

Breaking changes should have their own changeset with migration guidance.

When in doubt, choose the bump based on the user's or integrator's experience, not on implementation size. A one-line removal from a public API is usually `major`; a large internal refactor can be `none` if no published behavior changes.

## Lifecycle rules

Before adding a new changeset:

1. Read existing `.changeset/*.md` files.
2. Decide whether to create, update, merge, or delete.
3. Target package ids unless a configured group is the real release owner.
4. Keep unrelated changes in separate files.
5. Combine packages only when the release note would be the same.
6. Validate with `monochange step validate`, `monochange step diagnose-changesets --format json`, or `monochange step diagnose-changesets --format json`.

Delete or rewrite stale changesets when the code they describe is reverted before release. Merge near-duplicate changesets when several packages changed for the same outward behavior, but keep unrelated features separate even if they touched the same package.
