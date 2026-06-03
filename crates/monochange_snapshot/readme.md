# `monochange_snapshot`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_snapshot"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange**snapshot-orange?logo=rust)](https://crates.io/crates/monochange_snapshot) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange**snapshot-1f425f?logo=docs.rs)](https://docs.rs/monochange_snapshot/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_snapshot)](https://codecov.io/gh/monochange/monochange?flag=monochange_snapshot) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

`monochange_snapshot` owns normalized, framework-neutral snapshots for command and API surfaces that monochange can render, compare, and classify.

Reach for this crate when you need an agent-readable description of a CLI or another public surface without tying downstream tooling to a single command framework.

## Why use it?

- keep snapshot wire contracts separate from CLI implementation details
- render deterministic JSON for assistant workflows, review snapshots, and semantic diffing
- extract clap command definitions through the first supported extractor while leaving room for other frameworks
- classify command, option, positional, and value-contract changes as semver-impacting differences
- cap release impact for unstable or intentionally non-contractual command trees with `max_semver_bump`

## Version policy

Snapshot files carry a public `schema_version` value in the same `major.minor` style as other monochange durable schemas.

- The schema version is derived from the `monochange_snapshot` crate package version by dropping the patch component.
- The unreleased crate version `0.0.0` emits the first public snapshot schema version, `0.1`.
- Crate version `0.1.0` emits snapshot schema version `0.1`; crate version `1.0.0` emits `1.0`.
- Patch releases of this crate do not change emitted snapshot schema versions.
- Future breaking snapshot schema changes should advance the crate's major or minor version and add explicit migration support before old snapshots are rejected.

## Example

```rust
use clap::Command;
use monochange_snapshot::ClapCommandSurfaceExtractor;
use monochange_snapshot::CommandSurfaceExtractor;
use monochange_snapshot::SnapshotKind;

let command = Command::new("tool")
	.about("Example tool")
	.subcommand(Command::new("run").about("Run the tool"));
let extractor = ClapCommandSurfaceExtractor::new(&command);
let snapshot = extractor.extract();

assert_eq!(snapshot.kind, SnapshotKind::CliSurface);
assert_eq!(snapshot.provenance.extractor, "clap");
```

## Public entry points

- `CommandSnapshot` is the normalized CLI snapshot wire shape.
- `CommandNode::max_semver_bump` caps the release impact for changes at or below a command path.
- `CommandSurfaceExtractor` is the framework-neutral extraction trait.
- `ClapCommandSurfaceExtractor` extracts snapshots from clap command definitions.
- `snapshot_from_clap` provides a convenience clap extraction function.
- `diff_command_snapshots` classifies snapshot-to-snapshot CLI surface changes.

## Scope

- normalized CLI command, option, positional, and parser metadata
- deterministic JSON rendering for snapshot files
- clap-based extraction
- semver-oriented CLI surface diff classification
