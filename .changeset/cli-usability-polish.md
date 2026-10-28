---
monochange: change
monochange_config: fix
"@monochange/skill": fix
---

# Preview `check --fix`, print init next steps, and retire `monochange populate`

Three usability fixes:

- `monochange check --fix --dry-run` previews the fixes `--fix` would apply without writing any file. The summary says `would fix N files`, and the check still fails when errors remain on disk, so a dry run never silently passes a workspace that fixing would repair. `--dry-run` now requires `--fix`.
- `monochange init` prints the next commands to run (validate, discover, create, preview) instead of only naming the file it wrote.
- The legacy `monochange populate` command is removed. It has been a documented no-op since the default workflow set became empty; scripts calling it now get an unknown-command error and should call the steps they need directly.
- TOML syntax errors in `monochange.toml` render with the same source snippet semantic errors get: file, line and column, the offending line with a caret, and the parser's reason, instead of a flat `failed to parse` message.
