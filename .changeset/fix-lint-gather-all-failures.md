---
"monochange_config": patch
---

# Fix summary lint to report all violations at once

The `changesets/summary` lint rule used early returns that stopped checking after the first structural violation (wrong heading level, missing heading). This meant fixing one issue and re-running would reveal the next, requiring multiple iterations to surface all problems.

Now the rule collects all applicable violations in a single pass and reports them all together, matching user expectations that `mc check` surfaces every problem at once.
