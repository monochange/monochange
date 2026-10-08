# npm staged publishing

Status: in progress.

## Goal

Add npm staged publishing (`npm stage publish`) as a configurable publish flow so npm packages can require maintainer approval (with 2FA) before a release becomes installable, and document how it composes with trusted publishing.

## Research summary (2026-10)

npm now offers three distinguishable concepts:

- **Direct publishing** (`npm publish`): the version is installable the moment the command succeeds. Authentication comes from a login session (2FA), a granular/classic token, or a trusted-publishing OIDC exchange.
- **Trusted publishing** (OIDC, GA July 2025): an _authentication_ model. The registry trusts a CI identity (repository + workflow + optional environment) and exchanges a short-lived OIDC token for publish credentials; no stored token exists to steal. It says nothing about _when_ a version goes live. New trusted-publisher connections created after September 2026 default to allowing only `npm stage publish`; direct `npm publish` is a per-connection opt-in.
- **Staged publishing** (`npm stage publish`, GA May 2026): a _release_ model. The upload lands in a staging queue; a maintainer must approve with 2FA (`npm stage approve <stage-id>` or the Staged Packages tab on npmjs.com) before the version is installable. Staging itself requires no 2FA, so CI can run unattended.

The two compose: an OIDC workflow stages the artifact token-free, and a human authorizes exposure. npm calls this the strongest posture because it survives CI compromise — a hijacked workflow can stage but cannot approve (approval requires interactive 2FA and cannot use OIDC).

Forcing function: npm removes direct publishing with 2FA-bypass granular tokens in **January 2027**. After that, unattended publishing is either trusted publishing (direct, if allowed) or staged publishing with human approval. Stage-only granular tokens ("Read and write (stage only)") exist since September 2026 as the token-based migration path, and `npm stage publish` can create new packages since October 2026.

Command surface: `npm stage publish`, `npm stage list [<spec>]`, `npm stage view <stage-id>`, `npm stage download <stage-id>`, `npm stage approve <stage-id>`. Requires npm CLI ≥ 11.15.0 and Node ≥ 22.14.0. pnpm ≥ 11.3.0 mirrors the same subcommand shape with `pnpm stage publish` (same arguments as `pnpm publish`, plus `--dry-run`).

## Design decisions

- New option `flow` on npm publish settings, values `"direct"` (default, current behavior) or `"staged"`:

  ```toml
  [ecosystems.npm.publish]
  flow = "staged"
  ```

  Package-level `publish.flow` overrides the ecosystem default, matching every other publish option. `mode` is already taken by `builtin|external`, so the new key needs its own name; `flow` describes how the registry finalizes the release.
- **Default stays `direct` for now.** Flipping the default would silently hold every upgrading user's release in an approval queue and break post-publish automation; that is a deliberate major-version decision to revisit before npm's January 2027 cutover. npm's own precedent is the same shape: only _new_ trusted-publisher connections default to stage-only.
- `flow` is rejected with a config diagnostic on non-npm ecosystems (same pattern as the built-in registry override check).
- Staged release publishes run `<program> stage publish --access public [--provenance] [--dry-run]`; the program selection is unchanged (npm for trusted publishing, otherwise pnpm for pnpm workspaces) because both CLIs implement the `stage publish` subcommand.
- Placeholder publishing stays direct. Placeholders are a bootstrap utility (`0.0.0`), matching the existing rule that `publish.mode` does not affect `PlaceholderPublish`.
- Successful staged publishes report a dedicated `staged` status (not `published`) with approval guidance in the message, count in the summary (`staged`), and are treated as complete for `--resume`.
- Release records carry `flow` on each publication target (`#[serde(default)]`), keeping old records parseable and the release decision auditable.

## Non-goals

- No approval orchestration (`npm stage approve`) inside monochange: approval requires interactive 2FA, which CI and agents must not bypass. The publish report surfaces the approval commands instead.
- No staged-mode changes to `publish-readiness` registry probes or `placeholder-publish`.
- No default flip to staged (see design decisions).

## Affected files

- `crates/monochange_core/src/lib.rs`: `PublishFlow` enum, `PublishSettings.flow`, `PackagePublicationTarget.flow`.
- `crates/monochange_config/src/lib.rs`: `RawPublishSettings.flow`, merge + non-npm rejection in `normalize_publish_settings`.
- `crates/monochange_publish/src/lib.rs`: `PublishRequest.flow`, staged command construction, `PackagePublishStatus::Staged`, summary counter, resume handling.
- `crates/monochange/src/cli_runtime.rs`: report/summary rendering for staged outcomes.
- `crates/monochange/src/monochange.toml.template` + root `monochange.toml` annotations.
- Docs: `docs/src/guide/04-configuration.md`, `docs/src/guide/07-trusted-publishing.md`, `.templates/*.t.md` sources; regenerate JSON schema.
- Tests: `crates/monochange_config/src/__tests__/`, `crates/monochange_publish/src/__tests__/lib_tests.rs`, `crates/monochange/src/__tests__/package_publish_tests.rs`, integration fixture under `fixtures/tests/cli-output/`.

## Checklist

- [x] Failing tests first: config parse/override/reject, command construction, status/summary/resume.
- [x] Core types and serde defaults.
- [x] Config merge and validation.
- [x] Publish command construction and outcome recording.
- [x] CLI rendering.
- [x] Integration test: dry-run staged publish command snapshot.
- [x] Template, root config annotations, guide docs, skill docs, schema regeneration.
- [x] Changesets (feature, breaking API, website docs; `monochange_schema: major` drives the schema bump).
- [x] Schema contract advance to `0.10` with a no-op `0.9 → 0.10` migration edge and frozen `v0.10` assets.
- [x] `fix:all`, focused tests, `monochange step validate`, `monochange step affected-packages --from origin/main --verify`, docs/schema checks, patch coverage at 100%.

## Validation commands

```sh
devenv shell test:cargo --workspace --features monochange_publish
devenv shell fix:all
devenv shell lint:all
devenv shell monochange step validate
devenv shell monochange step affected-packages --from origin/main --verify
```

## Follow-up risks

- npm's `stage publish` flag surface (`--provenance`) is not fully documented; if the registry rejects provenance on staged uploads we may need to warn when both are configured.
- Stage-id capture: npm and pnpm print stage ids in different formats; monochange does not parse them, so approval guidance stays generic.
- Re-running a staged publish before approval stages again (staged versions are invisible to the registry version probe); document this in the guide.

## Schema version advance

`PackagePublicationTarget` gained `flow`, so both the config and release-record schema contracts changed. The published `v0.9` assets are frozen (they ship on the website and in releases since v0.16), so the schema version advances to `0.10`:

- `crates/monochange_schema/SCHEMA_VERSION` → `0.10`. The value is generated: the pipeline derives it from the crate version plus the pending changeset bump, so the changeset carries `monochange_schema: major` (on 0.x, `major` shifts to a minor-component bump: 0.9.2 → 0.10.0).
- New no-op migration edge `release_record_0_9_to_0_10` (the field is optional and defaults to `direct`), registered in the edges list and pinned by the migration tests.
- `docs/src/schemas/monochange.v0.10.schema.json` and `release-record.v0.10.schema.json` cut; `artifacts/0.10/` fixtures generated; the mdt `projectSchemaAssetIndex` block refreshed.
- The committed current+schema assets carried pre-existing generator drift (tab-indented output plus staleness); regenerating them also normalizes that drift, which is why the schema diff is large. `cargo xtask schema check` passes with the regenerated files and fails on `main` without them.
