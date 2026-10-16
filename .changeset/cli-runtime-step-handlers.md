---
monochange: refactor
---

# Split the CLI step dispatcher into per-step handlers

`execute_cli_command_with_options` had grown into a 1,085-line function whose step loop inlined the body of all 20 `CliStepDefinition` variants — cognitive complexity 449 with nesting up to 16 levels, and monostyle reported it as the single largest readability penalty in the repository. Each variant now dispatches to a dedicated `execute_<step>_step` function that receives a shared `StepExecutionState`, so the dispatcher reads as a table and each step's logic is readable in isolation. Behavior is unchanged: the CLI test suite and all snapshots pass without modification.

The repository also adopted monostyle for readability scoring (`lint:monostyle` gates `lint:all` at an overall score of 75 via `monostyle.toml`) and applied monostyle's 5,063 formatter-safe whitespace autofixes across the workspace, lifting the repository-wide readability score from 81.4 to 89.8.
