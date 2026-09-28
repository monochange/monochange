---
"monochange": major
"@monochange/skill": patch
---

# Serve the agent skill from the binary without the `skills` CLI

`monochange skill` no longer shells out to `npx`, `pnpm dlx`, or `bunx`. The skill bundle is embedded in the binary, so the command works offline and without Node tooling:

- `monochange skill` lists every bundled topic with its install path and description.
- `monochange skill read <topic>` prints one document verbatim to stdout, with no terminal rendering or added framing.
- `monochange skill install --dir <dir> [--force]` writes `SKILL.md` and every reference into an agent runtime skill directory and refuses to replace an existing skill unless `--force` is passed.

The forwarded-argument surface (`monochange skill --list`, `-a`, `-y`, and the other `skills add` flags) and the `MONOCHANGE_SKILL_SOURCE` and `MONOCHANGE_SKILL_RUNNER` environment variables are removed. `crates/monochange/skill/` is a committed copy of `packages/monochange__skill`, kept in sync by `scripts/docs/sync-skill.mjs` and verified by `docs:check`.

Migration: replace the forwarded-argument invocation with an explicit install directory, or read individual topics:

```bash
# Before
monochange skill -a pi -y

# After
monochange skill install --dir ~/.claude/skills/monochange
monochange skill read configuration
```
