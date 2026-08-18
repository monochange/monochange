---
monochange: patch
---

# Swap the load status indicator frames in place instead of reprinting the line

The load status indicator (spinner) previously rewrote its full line on every 90ms frame tick. When output is captured through a pty or log capture tool that renders carriage returns as new lines, every frame change produced a separate line, making logs very long.

The spinner now writes the full line only when the content changes — when it starts or after another writer (for example command output or publish progress) clears the line. Between those events it swaps only the frame character in place, so captured output stays compact while terminals still animate.
