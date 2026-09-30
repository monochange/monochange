---
monochange_config: patch
---

# Validate typed npm metadata files consistently

Typed npm `versioned_files` entries now accept custom `.json` files, including glob-selected deployment metadata. Validation previously rejected files such as `constraints.json` even though release preparation could update them.

Keep the explicit `type = "npm"` and select the fields to rewrite. Automatic package discovery still recognizes native npm manifests rather than treating every JSON file as a package.
