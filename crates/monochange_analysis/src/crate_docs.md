# `monochange_analysis`

<!-- {=monochangeAnalysisCrateDocs} -->

`monochange_analysis` orchestrates ecosystem-specific semantic analyzers over a git change frame.

Reach for this crate when you want to turn a git diff frame into package-scoped semantic analyses and suggested changesets without moving ecosystem logic into one place.

## Why use it?

- select the change frame to inspect with git-aware detection
- discover affected packages and load before/after package snapshots
- dispatch to the right ecosystem analyzer and return structured semantic diffs for CLI, MCP, and CI automation

## Best for

- suggesting changeset boundaries from pull-request or branch diffs
- feeding assistant workflows with structured semantic analyses
- sharing one analysis pipeline across every supported ecosystem

## Public entry points

- `ChangeFrame::detect(root)` selects the git frame to analyze
- `analyze_changes(root, frame, config)` returns package analyses and suggested changesets

Core contracts and semantic diff types live in `monochange_core`; ecosystem crates implement the analyzers.

<!-- {/monochangeAnalysisCrateDocs} -->
