# JSON Schema reference

monochange publishes JSON Schema assets with the documentation site. Every asset is available at a moving "current" URL that always describes the latest release, and at a stable versioned URL that never changes for a given contract version.

Use the moving URL for editor integration and local validation, and pin the versioned URL when generated files must validate against a fixed contract.

## Command snapshots

Normalized CLI command surface snapshots, as read by `monochange change classify` and written by CLI snapshot emitters.

| Asset   | URL                                                                                 |
| ------- | ----------------------------------------------------------------------------------- |
| Current | <https://monochange.github.io/monochange/schemas/command-snapshot.schema.json>      |
| v0.1    | <https://monochange.github.io/monochange/schemas/command-snapshot.v0.1.schema.json> |

- Contract version: `0.1`, also written in each document's `schema_version`.
- Related pages: [CLI snapshot emitters](cli-snapshot-emitters.md), [Package CLI registration](package-cli-registration.md).

Snapshot documents set `additionalProperties: false`, so unknown fields fail validation. That is deliberate: it turns a misspelled field in a foreign emitter into a local error instead of silently dropping data.

## Workspace configuration

JSON Schema for `monochange.toml` workspace configuration files.

| Asset   | URL                                                                           |
| ------- | ----------------------------------------------------------------------------- |
| Current | <https://monochange.github.io/monochange/schemas/monochange.schema.json>      |
| v0.6    | <https://monochange.github.io/monochange/schemas/monochange.v0.6.schema.json> |

Schema-aware TOML editors such as Taplo can opt in with a directive at the top of `monochange.toml`:

```text
#:schema https://monochange.github.io/monochange/schemas/monochange.schema.json
```

- Related pages: [Configuration](../guide/04-configuration.md).

## Release records

Durable commit-embedded release records written by `CommitRelease` and read by release discovery.

| Asset   | URL                                                                               |
| ------- | --------------------------------------------------------------------------------- |
| Current | <https://monochange.github.io/monochange/schemas/release-record.schema.json>      |
| v0.6    | <https://monochange.github.io/monochange/schemas/release-record.v0.6.schema.json> |

- Related pages: [Repairable releases](../guide/12-repairable-releases.md).

## Version namespaces

Contract versions are independent per artifact family:

- Snapshot documents carry the `monochange_snapshot` contract version, derived from that crate's package version.
- `monochange.toml` and release records carry the `monochange_schema` contract version.

A version bump only affects the artifact family it belongs to, so a new snapshot contract does not invalidate configuration or release-record assets. Versioned URLs are generated during release preparation; the moving aliases always describe the latest release.

## Regenerating assets

Committed schema assets are generated from the Rust wire types, so never hand-edit them.

```bash
schema:update            # regenerate current aliases and fixtures
schema:check             # verify committed assets match generated output
schema:release:update    # regenerate release assets, including versioned copies
schema:release:check     # verify release assets
```

`schema:check` runs as part of `lint:all` and in CI, so committed assets cannot drift from the types they describe. To validate a document against an asset locally, use any draft 2020-12 validator; the [CLI snapshot emitters](cli-snapshot-emitters.md#validate-before-you-register) page has copyable examples.

## Raw GitHub URLs

The same files are available from GitHub raw content, which is useful when you want to diff a pinned commit:

- <https://raw.githubusercontent.com/monochange/monochange/main/docs/src/schemas/command-snapshot.schema.json>
- <https://raw.githubusercontent.com/monochange/monochange/main/docs/src/schemas/monochange.schema.json>
- <https://raw.githubusercontent.com/monochange/monochange/main/docs/src/schemas/release-record.schema.json>
