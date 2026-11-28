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

`monochange ci check --workflow` now requires every `needs.<gate>.outputs.decisions` reference in an `if:` expression to include a status-check override plus both fail-safe guards. GitHub implicitly wraps a bare job `if:` in `success() && (...)`, so `needs.<gate>.result != 'success'` alone never runs when the gate fails. The canonical gate is:

```yaml
if: >-
  !cancelled()
  && (needs.gate.result != 'success' || needs.gate.outputs.run-all == 'true' || fromJSON(needs.gate.outputs.decisions)['test-windows'])
```

`always()` is accepted in place of `!cancelled()`. Comments, `env:` values, and `run:` scripts are not expressions and are ignored.
