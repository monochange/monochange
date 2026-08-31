---
"@monochange/skill": patch
monochange: minor
monochange_core: minor
---

# add `--format json-min` and guarantee plain text JSON output

Every command that accepts `--format json` (and every cli step `format` input) now guarantees plain-text JSON: no text colors, no background colors, and no other terminal styling leak into the output, even when color support would otherwise be detected. Machine consumers can pipe the output to a JSON parser without stripping escape sequences.

A new `json-min` choice renders the exact same data minified, with no indentation and no whitespace between tokens, which is convenient for piping into `--jq` filters, CI annotations, or log systems that prefer compact payloads.

```bash
# before
monochange run release --dry-run --format json
# → pretty-printed JSON (multi-line, indented)

# after — same data, one compact line
monochange run release --dry-run --format json-min
```

```bash
monochange versions list --format json-min
# {"core":"0.1.0"}
```

`json-min` is accepted anywhere `json` was: `analyze`, `check`, `lint`, `migrate`, `subagents`, `versions list/sync`, and the built-in step inputs (`[cli.*]` command inputs with `type = "choice"`, `choices = ["text", "json", "md"]` now also accept `"json-min"`):

```toml
[cli.release]
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json", "json-min", "markdown"], default = "markdown" },
]
```

Rejecting a JSON format no longer depends on terminal color detection either: styling is applied exclusively in `text` and `markdown` modes, so `NO_COLOR`-style env vars are no longer needed to keep JSON output clean in CI.
