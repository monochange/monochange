---
"@monochange/cli": docs
"@monochange/skill": docs
monochange: docs
---

# Organize migration guides per version and document upgrading to 0.11

The user guide gains a dedicated "Migration guides" part with one guide per version whose release contains breaking changes, listed newest first in `SUMMARY.md`. The guide for a version groups its upgrade steps by audience: CLI behaviour first, then configuration and machine-readable schemas, then library APIs. `docs/src/guide/migrations/index.md` states the convention, and the agent guidance in `docs/agents/changeset-quality.md` and `docs/agents/documentation.md` now requires the version guide to land in the same PR as the breaking change it documents.

The existing guides moved into the new structure: the knope migration now lives at `docs/src/guide/migrations/from-knope.md`, and the CLI command migration is version-scoped as "Upgrading to 0.9: the nested command API".

A new "Upgrading to 0.11" guide collects this release's upgrade steps: the human-first default output contract, `--quiet` no longer implying a dry run, `--jq` requiring JSON output, non-zero `check` exit status in every format, the longer default publish timeout with schema `v0.6`, the blocking trusted-publishing and publish-order readiness checks with readiness artifact `v3`, the tightened changeset summary rules, and the Rust API updates for `PackagePublishSummary`, structured release-note entries, `SemanticChange`, `CargoSemanticAnalyzer`, and exact `ChangeFrame::CustomRange` semantics.
