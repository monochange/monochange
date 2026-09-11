---
"monochange": patch
---

# Stop failing release pull request checks on consumed changesets

The changeset policy treated every changed `.changeset/*.md` path as an attached changeset it must find on disk, so a release pull request — whose whole point is deleting consumed changesets — failed with `attached changeset ... does not exist in the checked-out workspace`. The release-branch skip could not rescue it because pull-request CI runs on a detached checkout where the branch name is unknown.

Two behavior changes:

```bash
# before: release PR checks failed on their own consumed changesets
monochange step affected-packages
# ✖ config error: attached changeset `.changeset/example.md` does not exist in the checked-out workspace

# after: deleted changesets are reported as skipped warnings, not errors
monochange step affected-packages
# ✔ changeset verification passed: no configured packages were affected by the changed files
```

- When a changed changeset path no longer exists, the policy records a warning and moves on instead of failing. Coverage intent is still enforced: source changes without coverage still report `changed packages are not covered by attached changesets`.
- When the checked-out HEAD is detached (the standard pull-request CI checkout), the release pull-request branch-prefix skip now falls back to the `GITHUB_HEAD_REF` environment variable, so branches such as `monochange/release/*` are skipped exactly as they are locally.
