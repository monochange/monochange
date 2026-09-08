---
monochange_core: patch
---

# Describe every built-in workflow step input

`CliStepDefinition::step_inputs_schema()` now provides non-empty `help_text` for every built-in input. CLIs and agent snapshots built from the schema can explain flags such as `--package`, `--show-all`, `--otp`, `--resume`, and `--sync-provider` without maintaining a second help-text table.

```rust
let placeholder = monochange_core::all_step_variants()
	.into_iter()
	.find(|step| step.kind_name() == "PlaceholderPublish")
	.expect("placeholder step");

assert!(placeholder
	.step_inputs_schema()
	.iter()
	.all(|input| input.help_text.is_some()));
```
