# monochange reference

monochange is a CLI/tool harness for producing versioned packages from a monorepo. It connects package discovery, changeset intent, version planning, changelog rendering, versioned file updates, release records, source-provider releases, and package publishing workflows.

## Operating model

1. `monochange.toml` declares package ids, groups, ecosystems, versioned files, publishing settings, lints, and custom CLI workflows.
2. `.changeset/*.md` files declare release intent.
3. `PrepareRelease` computes package/group versions, updates files, and emits release-plan data.
4. Follow-up steps can commit, open release requests, tag releases, publish provider releases, publish package artifacts, and comment on issues.

## Inspecting a repository

```bash
mc help
mc step:validate
mc check
mc step:config
mc step:discover --format json
```

If a repository defines user workflows, `mc help` will show them under user-defined commands. The monochange repo currently defines `change`, `publish-check`, and `release`; those are configuration-defined, not universal built-ins.

## Version planning flow

```bash
mc step:validate
mc step:discover --format json
mc step:diagnose-changesets --format json
mc step:prepare-release --dry-run --format json
```

If configured aliases exist, users may prefer:

```bash
mc step:discover --format json
mc step:diagnose-changesets --format json
mc release --dry-run --format json
mc release --dry-run --diff
```

## Release mutation flow

A safe release workflow usually does this:

1. Validate and lint (`mc step:validate`, `mc check`).
2. Preview versioned files (`PrepareRelease` dry-run).
3. Apply `PrepareRelease` for real.
4. Run configured lockfile/schema/format commands.
5. Commit release changes with `CommitRelease`.
6. Open a release request or tag/publish from the release record.

Do not skip review before commit, tag, provider-release, or package-publish steps.

## Package publishing flow

Current built-in package publishing is release-record oriented:

```bash
mc step:publish-readiness --from HEAD --output readiness.json
mc step:placeholder-publish --from HEAD --output bootstrap.json
mc step:plan-publish-rate-limits --readiness readiness.json --format json
mc step:publish-packages --output publish-result.json
```

`mc step:publish-readiness`, `mc step:placeholder-publish`, `mc step:plan-publish-rate-limits`, and `mc step:publish-packages` are built in. Repositories may define shorter workflow aliases such as `mc publish-plan` or `mc publish`, but those names are not universal.

Use `mode = "external"` for private/custom registries or when existing CI handles package publication.

## MCP usage

Run `mc mcp` to expose structured tools to an MCP client. Use MCP for agent workflows that need JSON by default, especially validation, discovery, diagnostics, changeset creation, release previews, affected-package checks, and lint explanations.

Current tools are listed in `SKILL.md`.
