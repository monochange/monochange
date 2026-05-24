---
monochange_dart: patch
monochange: patch
---

# Allow Dart workspace resolution for internal dependencies

Dart linting now treats `resolution: workspace` as a valid internal package resolution mode, so versioned internal dependencies in `pubspec.yaml` files no longer fail the internal path dependency policy when Dart will resolve them from the workspace.
