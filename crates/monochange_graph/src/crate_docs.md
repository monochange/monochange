# `monochange_graph`

<!-- {=monochangeGraphCrateDocs} -->

`monochange_graph` turns normalized workspace data into release decisions.

Reach for this crate when you already have discovered packages, dependency edges, configuration, and change signals and need to calculate propagated bumps, synchronized version groups, and final release-plan output.

## Why use it?

- calculate release impact across direct and transitive dependents
- keep version groups synchronized during planning
- produce one deterministic release plan from normalized input data

## Best for

- embedding release-planning logic in custom automation or other tools
- computing the exact set of packages that need to move after a change
- separating planning logic from ecosystem-specific discovery code

## Public entry points

- `NormalizedGraph` builds adjacency and reverse-dependency views over package data
- `build_release_plan(workspace_root, packages, dependency_edges, defaults, version_groups, change_signals, compatibility_evidence, default_parent_bump, bump_propagations, strict_version_conflicts)` computes the release plan, honoring per-package and per-group `bump_propagation` declarations (most-specific-first: package beats group beats defaults)

## Responsibilities

- build reverse dependency views
- propagate release impact across direct and transitive dependents
- synchronize version groups
- calculate planned group versions

<!-- {/monochangeGraphCrateDocs} -->
