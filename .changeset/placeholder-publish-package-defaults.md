---
monochange: patch
---

# clarify placeholder publish package selection coverage

`monochange step placeholder-publish` continues to publish placeholders for every eligible publish-enabled package when no package filter is provided:

```bash
monochange step placeholder-publish
```

Passing `--package` narrows the placeholder publishing plan to the selected package ids only:

```bash
monochange step placeholder-publish --package core
```

This release strengthens the regression coverage for that behavior so automation can rely on the default all-packages mode while still using `--package` for targeted placeholder publishes.
