# Multi-package publishing

monochange can coordinate package publication after a release record exists. Publishing settings live in `[ecosystems.<name>.publish]` and `[package.<id>.publish]`.

Treat publishing as a separate phase from release preparation. Release preparation decides which versions should exist and records that decision; publishing checks registry state, handles first-time placeholder setup when needed, and then publishes only the packages selected by the release record and readiness artifacts.

## Recommended flow

1. Prepare and commit a release so a release record exists.
2. Run `monochange step publish-readiness --from HEAD --output readiness.json`.
3. Run any first-time bootstrap flow with `monochange step placeholder-publish` when packages are missing from registries.
4. Run configured publish planning and publish workflows if the repo defines them.
5. Store output artifacts so failed publishes can be resumed.

Readiness and publish artifacts are part of the safety model. They make a publish job auditable, let later steps confirm they are operating on the same release record, and help distinguish already-published packages from packages that still need work.

## Dry-run publish checks

<!-- {=publishDryRunChecks} -->

Publishing is the only release phase that cannot be rolled back. A broken release commit is fixed with a follow-up commit, and release tags can be deleted and re-created, but registry publications are permanent: when a multi-package publish fails partway, earlier packages are already live, later packages are missing, and cleanup usually means manual unpublishing or burned version numbers.

Run a dry-run publish check before anything mutates:

```bash
monochange step publish-packages --dry-run
```

The dry run resolves the same publish set and dependency-aware batches as a real publish, checks each selected version against its registry, and validates the configured publishing flow without mutating any registry. Repositories commonly expose it through a configured workflow command such as `monochange run publish-check` or through a lint script.

Use it at two checkpoints:

1. **A required CI job on every pull request.** Changes that would break publication fail CI instead of merging. This protects the default branch, but the pull request tree is not yet the release tree, so problems that only appear after versions change can still slip through.
2. **A simulated release commit.** In the same pull request, create the release commit locally without pushing (`monochange run release --commit`), run the dry-run publish check against that tree, then discard the commit. This validates the exact bumped versions, manifests, and changelogs the release will publish, and it is the strongest pre-merge signal.

For compiled ecosystems, also verify packages build from their packaged tarballs (for example `cargo package --workspace`) so a tarball that cannot compile fails CI instead of surfacing during publication.

Keep a dry-run publish check in the release workflow as the final gate before real publication. A problem that slips past CI then fails the release job before tags and hosted releases are created, so the fix is another commit instead of rolling back half-published registries.

<!-- {/publishDryRunChecks} -->

## Configuration pattern

```toml
[ecosystems.npm.publish]
enabled = true
mode = "builtin"
registry = "npm"
trusted_publishing = true

[ecosystems.npm.publish_order]
dependency_fields = ["dependencies", "devDependencies"]

[package."@acme/private-app"]
path = "apps/private"
type = "npm"
publish = { enabled = false }

[package."@acme/custom-registry"]
path = "packages/custom-registry"
type = "npm"
publish = { enabled = true, mode = "external" }
```

Built-in publishing targets canonical public registries. Use external mode for private registries, custom scheduling, or registry-specific orchestration that monochange should not manage.

Prefer ecosystem-level defaults when every public package publishes the same way, then override individual package tables for private packages, custom registries, or packages that need a different trust model.

Publish ordering uses ecosystem-specific dependency fields. npm defaults to `dependencies` and `devDependencies`; add `peerDependencies` or custom package.json fields through `[ecosystems.npm.publish_order].dependency_fields`, or remove `devDependencies` by setting the list to only `dependencies`. Cargo continues to order by `dependencies`, `dev-dependencies`, and `build-dependencies`. Deno uses `dependencies` and `imports`, Dart/Flutter use `dependencies` and `dev_dependencies`, Python uses `dependencies`, and Go uses `require` by default. Opt Python into `optional-dependencies` or `group.dependencies` only when optional extras or Poetry groups should block publish order.

## Safety

- Do not run real publish commands when the user only asked for a preview.
- Prefer `monochange step publish-readiness` before package publication.
- Prefer dry-run workflows such as a configured `monochange run publish-check` when available.
- Retain JSON artifacts from readiness, bootstrap, plan, and publish runs.
- Re-run readiness when manifests, lockfiles, publish config, registry auth mode, or package selection changes after an artifact was created.
- Use dry-run or readiness commands for investigation; reserve actual package publication for explicit release operations by an authorized maintainer or CI workflow.
