---
monochange: fix
---

# Keep advisory classification evidence out of release planning

Release planning escalated planned bumps from compatibility evidence even when `classification_enforced = false`, so a patch changeset released as `2.0.0` whenever the branch diff carried a rename the analyzer read as breaking, and the outcome differed by branch because the feature branch analyzed the branch range while the default branch analyzed the working directory.

Evidence for packages with `classification_enforced = false` now stays advisory in `preview`, `prepare`, and `run release` the way it already was in `change classify`: the changeset decides the bump and a plan warning records what the evidence suggested. The prepared-release cache fingerprint also includes the detected change frame, so a feature branch's plan is never reused on the default branch after a fast-forward merge leaves identical commits, changesets, and HEAD.
