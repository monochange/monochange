---
"monochange_config": patch
---

Fix summary lint rule to report all violations instead of stopping at the first failure

The `changesets/summary` lint rule previously used early returns that stopped checking after the first structural violation (wrong heading level, missing heading). This meant fixing one issue (e.g. heading level) and re-running would reveal the next issue (e.g. max length), requiring multiple iterations.

Now the rule collects all applicable violations in a single pass and reports them all together, matching user expectations that `mc check` surfaces every problem at once.
