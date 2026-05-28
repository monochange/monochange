---
monochange: fix
---

# Fix ANSI color bleeding between CLI step outputs

When subprocess output (e.g. from cargo, git) contained ANSI color codes without trailing reset sequences, the color state would leak into subsequent step output, spinner rendering, and progress lines. This caused brownish/yellowish color bleeding visible when Prepare Release ran before other steps.

Added `\x1b[0m` (ANSI reset) to all output paths:

- Raw subprocess log lines in `log_command_output`
- Line-clear sequences in `print_line` and `stop_spinner`
- Spinner rendering in the animation thread
- Phase timing detail lines
