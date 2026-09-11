---
"@monochange/skill": patch
---

# Correct the permissions needed to publish the classification comment

The bundled classification guidance told agents to request `issues: write` with `pull-requests: read`. That combination cannot create a pull request comment. The API answers `403 Resource not accessible by integration`, the action downgrades the error to a warning, and the job still passes, so the report never reaches the pull request.

Request the pull requests write scope instead. Comment creation is governed by the scope of the resource that owns the comment, and a pull request comment belongs to the pull request:

```yaml
permissions:
  contents: read
  pull-requests: write
```

Watching for the warning is the only signal that the comment was skipped; the job summary and action outputs are unaffected either way.
