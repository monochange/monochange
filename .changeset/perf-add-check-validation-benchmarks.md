---
"monochange": patch
---

Add criterion benchmarks for check/validation pipeline performance

Adds `check_validation` benchmarks to measure:

- `validate_versioned_files_content_with_config` with increasing package
  counts (10, 50, 100) to verify glob deduplication stays O(N)
- `validate_workspace_with_config` with pre-loaded config vs reloading
- Config load vs validate comparison to quantify the `_with_config`
  optimization benefit (60µs vs 5.7ms — 95% reduction)