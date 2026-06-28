---
monochange: patch
---

# remove placeholder publish package default coverage note

The previous release-note entry for `monochange step placeholder-publish` described strengthened regression coverage rather than a user-facing CLI behavior change. This update removes that test-only coverage change while leaving the command behavior unchanged.

The command still checks all eligible publish-enabled workspace packages when no package filter is provided:

```bash
monochange step placeholder-publish
```

Use `--package` when you want to narrow the placeholder publish check to specific package ids:

```bash
monochange step placeholder-publish --package core
```
