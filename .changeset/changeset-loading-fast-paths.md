---
monochange_config: perf
monochange_cargo: perf
monochange: perf
---

# Load changesets without redundant canonicalization and re-parsing

Loading a changeset file rebuilt a `ChangesetLoadContext` that canonicalized the workspace root and every package manifest path — four `realpath` syscalls per package per file — so batch loading cost grew with packages × changesets. The context now canonicalizes the root once, resolves manifest paths with a `strip_prefix` fast path, and borrows configured change-type names instead of cloning them per build. Measured with the `cli_commands` benchmarks, changeset loading drops 84–96% across 5/20/50-package workspaces and `prepare_release` dry runs drop 6–44%.

Changeset parsing also stops allocating two full copies of every file for line-ending normalization (a `Cow` borrows LF-only text, the common case), validation no longer rebuilds the package and group lookups for every changeset file, and configured Cargo packages are discovered with one manifest read and parse instead of three.
