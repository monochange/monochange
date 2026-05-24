---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_dart: minor
monochange_npm: minor
monochange_publish: minor
monochange_schema: minor
---

# Resilient discovery and Dart/Flutter ecosystem unification

#### Discovery no longer crashes on unfamiliar ecosystems

`mc init` and `discover_all` now gracefully handle errors from individual ecosystem adapters. When an adapter (e.g. npm) fails in a monorepo that doesn't use that ecosystem (e.g. a Dart monorepo), the error is logged as a warning and discovery continues with remaining adapters instead of aborting. The npm adapter's `expand_member_patterns` also guards against workspace glob patterns that resolve to directories without a `package.json`.

#### Flutter merged into the Dart ecosystem

`Ecosystem::Flutter` and `PackageType::Flutter` have been removed. Flutter packages use `Ecosystem::Dart` with an `is_flutter` metadata flag on the package record, since Flutter and Dart share `pubspec.yaml`, `pub.dev`, and the same tooling. The publish and lockfile commands now check this metadata to choose `flutter pub get`/`flutter pub publish` vs `dart pub get`/`dart pub publish`. Config files that use the string `"flutter"` are deserialized to `Ecosystem::Dart` or `PackageType::Dart` for backward compatibility.

```toml
# Before (still works, maps to dart ecosystem):
[[packages]]
type = "flutter"

# After (preferred):
[[packages]]
type = "dart"
```
