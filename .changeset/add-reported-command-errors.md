---
monochange_core: patch
---

# Preserve structured command output when an operation fails

`MonochangeError::Reported` lets a command return a machine-readable result on stdout and a separate failure diagnostic. Use `reported_output()` at the process boundary to write the result before returning a non-zero exit status.

```rust
let error = MonochangeError::Reported {
	output: json_report,
	diagnostic: "check failed: 1 error".to_string(),
};

assert_eq!(error.reported_output(), Some(expected_json));
```

Callers that only use `Display` or `render()` continue to receive the diagnostic without the attached output.
