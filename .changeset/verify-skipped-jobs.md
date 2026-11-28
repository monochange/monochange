---
monochange: fix
monochange_app: patch
---

# Reject skipped jobs as verification evidence

`monochange ci verify` no longer records `true_negative` for a job that never ran. A `skipped` or `cancelled` conclusion proves nothing about the gate decision, so the command now exits non-zero and writes no verification record; `success` remains the only conclusion that verifies a skip. This keeps the recorded skip rate and analytics honest instead of counting non-runs as successful gates.

**Before:**

```bash
monochange ci verify --job docs --conclusion skipped
# classification: true_negative
```

**After:**

```bash
monochange ci verify --job docs --conclusion skipped
# job `docs` concluded `skipped`; only `success` or `failure` verifies a gate decision, so no verification record was written
```

`monochange ci check --workflow` now also rejects a `needs.<gate>.outputs.decisions` reference that omits `needs.<gate>.result != 'success'` or `needs.<gate>.outputs.run-all == 'true'`. Without both guards, a failed gate skips the dependent job and the skipped required check counts as passing.
