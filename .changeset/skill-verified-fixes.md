---
"monochange": fix
"@monochange/skill": fix
---

# Fix verified factual errors in the monochange agent skill

The bundled skill (`packages/monochange__skill`, served by `monochange skill read`) taught several commands and fields that do not match the current binary. Each fix was reproduced against the debug binary before editing:

- **`monochange versions`**: the skill now documents the supported subcommands. `monochange versions list` is a read-only inventory (`--format text|json|json-min`), and `monochange versions sync` rewrites internal dependency constraints (`--dry-run`, `--format`, `--strategy default|exact|caret|compatible`). `--strategy` belongs to `sync` only. Bare `monochange versions` is deprecated and prints a warning, so skill examples no longer use it.
- **Cross-stream changesets**: `monochange step validate` and `monochange check` both pass for a file that mixes `default`-stream and `user`-stream targets; only a release plan command rejects it. The skill now names `monochange preview` (or `monochange prepare --dry-run` / `monochange step prepare-release --dry-run`) and states the observed failure: exit 1 with `changeset targets resolve to multiple changelog streams: <streams>; split the changes into one file per stream`.
- **Compatibility field**: replaced the stale `compatibilityEvidence` name. The release plan exposes `compatibility_evidence`, and the classification report's verdict is `decision.compatibility_impact`.
- **Top-level command surface**: `commands.md` and `SKILL.md` now document the short built-ins (`create`, `discover`, `config`, `preview`, `prepare`, `affected`, `diagnose`, `next`, `next-versions`, `publish packages|readiness|placeholder`, `versions list|sync`) with the step each one runs, the preferred order (configured workflow, then short built-in, then `monochange step <name>`), and the steps that remain step-only (`validate`, `commit-release`, `tag-release`, and others). The generated inventory keeps owning the clap-literal and step-name sections.
- **Python and Go version writing**: configuring a `python` package does not rewrite its own `[project].version` unless a `versioned_files` entry lists `version` in `fields`. Without it, `monochange prepare` plans the version and rewrites internal constraints but leaves the manifest stale. Go modules carry no version field; `go` packages resolve their baseline from release tags, so `tag = true` plus `initial_version` is required and no `versioned_files` entry can write a module version.
