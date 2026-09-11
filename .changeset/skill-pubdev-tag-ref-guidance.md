---
"@monochange/skill": patch
---

# Document the pub.dev tag-ref requirement for workflow_dispatch publishing

The trusted-publishing skill now records that pub.dev requires every publish run — including `workflow_dispatch` runs — to carry a `refs/tags/<tag-pattern>` ref matching the published version, that the "Enable publishing from `workflow_dispatch` events" checkbox only relaxes the event name, and how to dispatch on a release tag with `gh workflow run <workflow>.yml --ref <tag>`.
