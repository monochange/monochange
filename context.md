Written findings to `context.md`. Here's the summary:

---

**The color bleed source is `CliProgressReporter::log_command_output`** in `cli_progress.rs` (line 414-454).

**The exact code path:**

1. `execute_command_step` spawns a child process (e.g., cargo, npm)
2. A streaming loop at `cli_runtime.rs:1946` reads raw stdout/stderr chunks from the child process
3. Each chunk is converted via `String::from_utf8_lossy` and passed directly to `progress.log_command_output()`
4. `log_command_output` iterates each line and embeds it raw into a formatted progress line via `self.print_line(&format!("  {} {} {}", ...pipe, ...label, line))`
5. The `line` variable contains unmodified ANSI escape codes from the child process

**The bug:** Child process ANSI color codes (like `\x1b[33m` for cargo warnings) pass through to stderr without being stripped or reset. If a child process doesn't emit a final `\x1b[0m` at the end of its output, the color state bleeds into the next step's spinner, step indicators, and progress lines.

**Properly handled code (no issues):**

- `paint_text()` — each fragment individually gets `\x1b[0m` reset
- `push_ansi_style()` in `release_artifacts.rs` — same, per-line reset
- `print_line()` — clears with `\r\x1b[2K` before writing
- Final stdout output — plain text, no ANSI codes
