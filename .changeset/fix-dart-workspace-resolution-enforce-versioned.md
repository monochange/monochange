---
monochange_dart: patch
monochange: patch
---

# Enforce version constraints for Dart workspace resolution internal deps

The `dart/internal-path-dependency-policy` lint rule now enforces version
constraints (not `path:` references) when a pubspec declares `resolution:
workspace`. Dart workspace resolution resolves versioned internal dependencies
to local workspace packages automatically, so `path:` references are redundant
and can cause publishing issues.

**Before:** With `resolution: workspace`, internal deps using either `path:` or
version constraints would pass the lint.

**After:** With `resolution: workspace`, internal deps must use version
constraints — `path:` references now produce a lint failure with the message
"use version constraints (not `path:`) when resolution is workspace".