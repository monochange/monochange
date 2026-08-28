---
monochange_core: feat
monochange_config: feat
monochange_schema: docs
"@monochange/skill": docs
---

# accept native TOML booleans and numbers in CLI step inputs

CLI step input overrides in `monochange.toml` no longer need to be strings. Authoring `{ draft = true }` (boolean) and `{ jobs = 4, ratio = 2.5 }` (numbers) in a step `inputs` map now parses, and the JSON Schema for the config accepts all three literal shapes:

- booleans keep their native type through parsing and are stringified to `"true"`/`"false"` when the step runs (unchanged behavior, now covered by tests)
- numbers are coerced to their string form at parse time, so `{ jobs = 4 }` is exactly `{ jobs = "4" }` once the step runs
- input declarations already accepted boolean and number `default` literals; that behavior is now covered by tests and documented
- the generated JSON Schema for step input overrides accepts `string`, `boolean`, and `number` values
- the configuration guide, the Command step reference, and the bundled skill gain an explicit interactive `Command` step example: declare an `interactive` boolean input, pass it to the step, and run the workflow with `--interactive` so the command inherits stdio and owns the terminal
