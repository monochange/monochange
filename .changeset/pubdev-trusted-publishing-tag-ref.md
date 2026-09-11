---
"monochange_publish": fix
---

# Fail fast when pub.dev trusted publishing runs from a non-tag ref

pub.dev rejects trusted-publishing uploads from any GitHub Actions run whose ref is not `refs/tags/<tag-pattern>` matching the published version — including `workflow_dispatch` runs dispatched on a branch, even when "Enable publishing from `workflow_dispatch` events" is enabled on pub.dev. That checkbox only allows the event name; the tag-ref requirement stays in force for every event.

Trusted pub.dev publishes from a non-tag run ref (`refs/heads/*`, `refs/pull/*`) used to log a warning and still mint a fresh OIDC token, so branch-ref dispatches failed late with an opaque registry authorization error after the token mint. They now fail before any token is minted or any dart command runs, with the working recipe: push the release tag, then `gh workflow run <workflow>.yml --ref <tag>` (or publish from the tag-push event). Runs that carry a real `PUB_TOKEN` credential keep publishing unchanged, and tag-ref runs are unaffected.
