---
monochange_dart: patch
---

# Add environment constraints to Dart placeholder manifests

Generated Dart and Flutter placeholder `pubspec.yaml` files now reuse the source package's `environment` block when available, falling back to safe Dart/Flutter SDK constraints when it is missing.
