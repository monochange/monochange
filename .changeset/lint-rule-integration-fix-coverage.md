---
monochange: patch
---

# Lock every lint rule's autofix behavior behind integration tests

Every lint rule now has integration coverage that runs `monochange check --fix` against file fixtures and snapshots the fixed manifests, so a rule that deletes or corrupts manifest content can no longer land unnoticed. The fixtures cover all cargo, npm, dart, and changeset rules, including the rules without autofixes whose diagnostics must persist.
