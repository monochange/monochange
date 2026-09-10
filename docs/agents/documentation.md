# Documentation workflow

Shared documentation blocks live in `.templates/` and are synchronized with `mdt`.

- Treat `AGENTS.md` as the table of contents, not the full manual.
- Keep `ARCHITECTURE.md`, `docs/agents/*.md`, and `docs/plans/` as the repo-local system of record for agent-facing guidance.
- Edit provider blocks in `.templates/` when one change should update multiple docs.
- Do not embed shared markdown in Rust source with `include_str!` (for example `#![doc = include_str!("crate_docs.md")]`); embed the mdt consumer block directly in the doc comments instead, so one provider block reaches rustdoc, READMEs, and the docs site and package tarballs never depend on a stray markdown file:

  ```rust
  //! <!-- {=monochangeCoreCrateDocs|trim|linePrefix:"//! "} -->
  //! <!-- {/monochangeCoreCrateDocs} -->
  ```
- Run `docs:update` after changing shared docs or consumer blocks.
- Run `docs:check` before opening a PR to confirm shared blocks are synchronized and agent-facing documentation stays fresh.
- For complex or multi-step work, create or update a plan under `docs/plans/active/`, then move it to `docs/plans/completed/` when the work lands.
- Migration guides live in `docs/src/guide/migrations/`, one file per version with breaking changes (for example `0.11.md`), listed newest first under the "Migration guides" part in `SUMMARY.md`. Update the version guide in the same PR that introduces the breaking change.
- Treat `docs/` as a product surface when behavior changes.
