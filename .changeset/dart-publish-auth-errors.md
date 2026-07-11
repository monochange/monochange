---
monochange_publish: patch
---

# report Dart trusted publishing authentication failures

Built-in package publishing now runs registry publish commands with stdin closed so commands cannot wait indefinitely for interactive authentication prompts in CI. This helps `dart pub publish --force` fail fast when pub.dev credentials or trusted-publishing context are missing instead of waiting until the workflow timeout.

When pub.dev publishing reports an authentication or credential error, monochange now appends guidance for the common trusted-publishing setup issues:

```text
pub.dev publishing could not authenticate non-interactively. If this package uses trusted publishing, verify the GitHub workflow has `id-token: write`, runs with the GitHub Actions environment configured on pub.dev, matches the package repository and tag/event policy, and runs `dart-lang/setup-dart` before `dart pub publish`.
```

Token fallback workflows are also pointed to the explicit pub token setup command:

```bash
dart pub token add https://pub.dev --env-var PUB_TOKEN
```
