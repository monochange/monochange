# Migration guides

Migration guides exist for the versions whose changes require you to update code, configuration, or automation. Each guide covers one version jump and stays focused on the actions you must take. The complete change list for every version lives in [changelog.md](../../../changelog.md).

## When a version gets a guide

A version gets a guide when its release contains at least one change that breaks how people use, configure, or automate monochange. Patch releases and feature releases without breaking changes do not get a guide. Rust-only breaking changes are a section inside the same version guide rather than a separate document.

## How the guides are organized

- One file per version at `docs/src/guide/migrations/<version>.md`, for example `0.11.md`.
- Listed newest first in the `SUMMARY.md` "Migration guides" part.
- Entries are grouped by audience: CLI behaviour first, then configuration and machine-readable schemas, then library APIs.
- Every entry states who is affected, what changed, and the exact update step with before and after examples.
- Breaking changesets reference the version guide so release notes link readers to the upgrade steps.

## Guides

- [Upgrading to 0.11](0.11.md)
- [Upgrading to 0.9: the nested command API](0.9-cli-command-api.md)
- [Migrating from knope](from-knope.md) — for repositories coming from the knope release tool rather than upgrading monochange.

Versions older than 0.9 predate the migration-guide convention; their breaking changes are documented in the changelog.
