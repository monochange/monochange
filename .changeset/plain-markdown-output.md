---
"monochange": fix
---

# Keep `--format markdown` output free of terminal colours

`--format markdown` output coloured headings and code spans with ANSI escape codes when stdout was a terminal, so copying it into a pull request or file pasted escape codes. Markdown output is now always plain markdown.

`--format markdown` is also accepted by every command that offers `--format md`, such as `monochange affected` and `monochange step diagnose-changesets`, instead of failing with `invalid value 'markdown'`.
