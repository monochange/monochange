---
monochange: fix
---

# Make `--help` pages consistent and fix misleading CLI messages

Help pages for `next`, `create`, `preview`, `prepare`, `affected`, `diagnose`, `discover`, `config`, and the `step` and `publish` subcommands now list one `Options` block (with `-h, --help`), use `[OPTIONS]` in the usage line, and include examples. `--dry-run` is listed only on commands that can write files or contact a service. It is still accepted everywhere, so existing scripts keep working. `init`, `skill`, `versions`, `publish`, and `change classify` print the same compact layout as every other page; their longer explanations now follow the options.

`--format` reads the same everywhere: `--format <FORMAT>  Output format [possible values: text, json, json-min, md]`, with `markdown` still accepted as a spelling of `md`. Commands without Markdown output list `text, json, json-min`. Every `monochange step --help` entry now has its own description instead of "Run the built-in … release workflow step", and the no-op `monochange populate` is hidden from help but still runs.

Messages that pointed people the wrong way are fixed:

- `monochange next` with no changesets suggests `monochange create` instead of `monochange change`, which needs a subcommand.
- `monochange check` reports each lint suite's own rule count instead of the total for every suite.
- `monochange publish readiness` without `--from` names the missing option and exits with status `2`:

  ```text
  error[cli.usage]: missing required option `--from <ref>`; pass the release commit or tag to check, for example `--from HEAD`
  ```

- `monochange lint explain <unknown>` points at `monochange lint list`, and `monochange notes` lists the target ids to pass to `--target` when an output covers several. Both also exit with status `2`.
- `check`, `next`, `preview`, `prepare`, and the other release commands warn when no `monochange.toml` exists instead of silently planning with defaults.
- `monochange subagents --help` no longer lists `monochange` twice in the preferred-CLI order.
- When GitHub rejects the review-request lookup in `diagnose`, the warning shows GitHub's status and message (for example `status 401; Bad credentials`) instead of `config error: … GitHub`.
