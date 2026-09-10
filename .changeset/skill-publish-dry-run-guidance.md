---
"@monochange/skill": minor
---

# Recommend dry-run publish checks before any release merges

The skill now treats the dry-run publish check as a first-class release safety practice, so agents and CI authors following monochange guidance stop avoidable partially-published releases: a multi-package publish that fails partway leaves earlier packages live on their registries while tags and hosted releases still have to be rolled back by hand.

The multi-package publishing module documents two checkpoints and the exact commands:

```bash
# required CI job on every pull request
monochange step publish-packages --dry-run

# strongest pre-merge signal: simulate the release commit without pushing,
# then validate the bumped tree
monochange run release --commit
monochange step publish-packages --dry-run
```

The skill's source-of-truth rules now tell agents to gate CI on the dry-run publish check, and the publishing guide recommends keeping the same check in the release workflow as a final gate so a late failure happens before tags and hosted releases exist.
