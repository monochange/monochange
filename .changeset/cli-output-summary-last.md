---
monochange: change
---

# End every text result with its summary line

Long-running commands such as `monochange publish`, `monochange publish placeholder`, `monochange prepare`, and `monochange check` now read from top to bottom: the plan and its details first, then the log of the work, then the detailed results, and finally one summary line. The answer to "what happened?" is always the last line in the terminal instead of being buried above tables and rate-limit notes.

- Every text result moves its summary line (for example `✔ Published 8 placeholder packages · 16 already published`) from the top of stdout to the bottom. Sections are ordered from supporting detail to the rows that need attention, so failures, blocked packages, lint errors, and policy errors sit right above the summary.
- Publishing prints the rate-limit plan on stderr directly under the run's opening line, before any package is published. Each registry shows its pending packages, batches, and documented limit; batch membership is listed only when a registry needs more than one batch. The publish result no longer repeats the plan and keeps only rate-limit warnings. `monochange step plan-publish-rate-limits` still prints the full plan as its result.
- Publish results group packages under `Would publish`, `Published`, `Staged awaiting approval`, `Blocked`, and `Failed` headings, and the separate `Checked N packages…` sentence is folded into the summary line.
- `--progress-format json` emits a `publish_plan` event with the rate-limit report right after `publish_run_started`.
- `monochange lint explain <id>` keeps its title at the top because it describes a rule rather than reporting a run.

```text
$ monochange publish placeholder
▶ Publishing 24 packages (Placeholder) across 📦 npm, 🎯 dart
  › npm      5 pending · 1 batch · no documented limit · low confidence
  › pub_dev  3 pending · 1 batch · limit 12 per 24h · medium confidence
✔ 📦 npm @skribble.dev/react published 0.0.0 to npm
…
✓ Publish complete: 8 published, 16 skipped
✔ publish placeholder packages  29.5s

Published (8)
  @skribble.dev/react  0.0.0  npm
  …

✔ Published 8 placeholder packages · 16 already published
```

Scripts that read the first line of a text result should read the last line instead, or switch to `--format json`, which is unchanged.
