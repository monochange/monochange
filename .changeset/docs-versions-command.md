---
"@monochange/skill": patch
monochange: patch
---

# Document `mc versions` command with full ecosystem coverage

Update documentation and skill to accurately describe the `mc versions` command:

- **All ecosystems supported**: Cargo, Dart, Deno, Go, npm, and Python
- **Usage examples**: `--dry-run`, `--format json`, `--strategy exact/caret/compatible`
- **Ecosystem details**: Each adapter's manifest scanning behavior documented

Fix incorrect reference in start-here guide that described `mc versions` as read-only (it actually writes to manifests).
