---
monochange_core: patch
monochange: patch
---

# Add `-i` short flag and hide the loader during interactive prompts

`monochange create` (and every schema-built step command) now accepts `-i` as a short flag for `--interactive`, matching the documented example.

The progress spinner is now paused while the interactive change wizard owns the terminal, so the loader no longer animates over the selector UI. It restarts while the change file is written, so the loader only shows while work is actually being done.

Custom commands can now run interactive tools in `Command` steps by passing an `interactive` input:

```toml
[cli.wizard]
inputs = [
	{ name = "interactive", type = "boolean", default = false, short = "i" },
]
steps = [
	{ name = "run wizard", type = "Command", command = "my-tui", inputs = ["interactive"] },
]
```

Interactive `Command` steps run with inherited stdio so the tool can read from stdin and render its own UI, and the spinner is suppressed for the step. Output is not captured for interactive steps.
