# JSON Schema reference

<!-- {=projectSchemaAssetIndex} -->

**`classification.schema.json`**

- Current: <https://monochange.github.io/monochange/schemas/classification.schema.json>
- v0.1: <https://monochange.github.io/monochange/schemas/classification.v0.1.schema.json>
- v0.2: <https://monochange.github.io/monochange/schemas/classification.v0.2.schema.json>

**`command-snapshot.schema.json`**

- Current: <https://monochange.github.io/monochange/schemas/command-snapshot.schema.json>
- v0.1: <https://monochange.github.io/monochange/schemas/command-snapshot.v0.1.schema.json>

**`monochange.schema.json`**

- Current: <https://monochange.github.io/monochange/schemas/monochange.schema.json>
- v0.0: <https://monochange.github.io/monochange/schemas/monochange.v0.0.schema.json>
- v0.1: <https://monochange.github.io/monochange/schemas/monochange.v0.1.schema.json>
- v0.2: <https://monochange.github.io/monochange/schemas/monochange.v0.2.schema.json>
- v0.3: <https://monochange.github.io/monochange/schemas/monochange.v0.3.schema.json>
- v0.4: <https://monochange.github.io/monochange/schemas/monochange.v0.4.schema.json>
- v0.5: <https://monochange.github.io/monochange/schemas/monochange.v0.5.schema.json>
- v0.6: <https://monochange.github.io/monochange/schemas/monochange.v0.6.schema.json>
- v0.7: <https://monochange.github.io/monochange/schemas/monochange.v0.7.schema.json>
- v0.8: <https://monochange.github.io/monochange/schemas/monochange.v0.8.schema.json>

**`release-record.schema.json`**

- Current: <https://monochange.github.io/monochange/schemas/release-record.schema.json>
- v0.0: <https://monochange.github.io/monochange/schemas/release-record.v0.0.schema.json>
- v0.1: <https://monochange.github.io/monochange/schemas/release-record.v0.1.schema.json>
- v0.2: <https://monochange.github.io/monochange/schemas/release-record.v0.2.schema.json>
- v0.3: <https://monochange.github.io/monochange/schemas/release-record.v0.3.schema.json>
- v0.4: <https://monochange.github.io/monochange/schemas/release-record.v0.4.schema.json>
- v0.5: <https://monochange.github.io/monochange/schemas/release-record.v0.5.schema.json>
- v0.6: <https://monochange.github.io/monochange/schemas/release-record.v0.6.schema.json>
- v0.7: <https://monochange.github.io/monochange/schemas/release-record.v0.7.schema.json>
- v0.8: <https://monochange.github.io/monochange/schemas/release-record.v0.8.schema.json>

<!-- {/projectSchemaAssetIndex} -->

Snapshot documents set `additionalProperties: false`, so unknown fields fail validation. That is deliberate: it turns a misspelled field in a foreign emitter into a local error instead of silently dropping data.

Schema-aware TOML editors such as Taplo can opt in to the configuration schema with a directive at the top of `monochange.toml`:

```text
#:schema https://monochange.github.io/monochange/schemas/monochange.schema.json
```

## Version namespaces

Contract versions are independent per artifact family:

- Snapshot documents carry the `monochange_snapshot` contract version, derived from that crate's package version.
- `monochange.toml` and release records carry the `monochange_schema` contract version.

A version bump only affects the artifact family it belongs to, so a new snapshot contract does not invalidate configuration or release-record assets. Versioned URLs are generated during release preparation; the moving aliases always describe the latest release.

Only versions on the breaking axis are listed above. While a family's major version is `0`, every minor bump may break consumers, so each `0.N` is published. From `1.0` onward only a major bump may break consumers, so only `N.0` is published. Intermediate releases keep their moving alias and remain reachable by their exact URL.

## Related pages

- Command snapshots: [CLI snapshot emitters](cli-snapshot-emitters.md), [Package CLI registration](package-cli-registration.md).
- Configuration: [Configuration](../guide/04-configuration.md).
- Release records: [Repairable releases](../guide/12-repairable-releases.md).

## Regenerating assets

Committed schema assets are generated from the Rust wire types, so never hand-edit them.

```bash
schema:update            # regenerate current aliases and fixtures
schema:check             # verify committed assets match generated output
schema:release:update    # regenerate release assets, including versioned copies
schema:release:check     # verify release assets
```

`schema:check` runs as part of `lint:all` and in CI, so committed assets cannot drift from the types they describe. The version list above is generated from the committed versioned assets by `scripts/schema-versions.ts`, so it stays current without hand edits. To validate a document against an asset locally, use any draft 2020-12 validator; the [CLI snapshot emitters](cli-snapshot-emitters.md#validate-before-you-register) page has copyable examples.

## Raw GitHub URLs

The same files are available from GitHub raw content, which is useful when you want to diff a pinned commit:

- <https://raw.githubusercontent.com/monochange/monochange/main/docs/src/schemas/command-snapshot.schema.json>
- <https://raw.githubusercontent.com/monochange/monochange/main/docs/src/schemas/monochange.schema.json>
- <https://raw.githubusercontent.com/monochange/monochange/main/docs/src/schemas/release-record.schema.json>
