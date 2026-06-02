---
monochange_schema: major
monochange_config: minor
monochange_core: minor
monochange_analysis: minor
monochange_publish: minor
monochange_test_helpers: patch
monochange: minor
---

# Use snake_case for durable JSON schemas

Normalize durable monochange JSON schemas, release records, and CLI/report outputs to snake_case while preserving migration support for legacy camelCase release records.

```json
{
	"schema_version": "0.4",
	"kind": "monochange.release_record"
}
```
