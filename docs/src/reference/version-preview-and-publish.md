# Version preview and `monochange publish`

Two built-in commands shorten the most common release questions: what version comes next, and how do I run a publish step.

## Check the next version

`monochange next` prints the planned next version for every release group and every package that releases independently. It reads pending changesets and nothing else.

```bash
monochange next
```

```text
group versions:
- sdk: 1.1.0
package versions:
- cargo:crates/sdk-a/Cargo.toml: 1.1.0
- cargo:crates/sdk-b/Cargo.toml: 1.1.0
- cargo:crates/tool/Cargo.toml: 1.0.1
```

Group members report the group version because they share one release identity. `tool` releases on its own, so it reports its own version.

Use `--format` for structured output:

```bash
monochange next --format json
monochange next --format json-min
monochange next --format md
```

```json
{
	"packages": {
		"cargo:crates/sdk-a/Cargo.toml": "1.1.0",
		"cargo:crates/sdk-b/Cargo.toml": "1.1.0",
		"cargo:crates/tool/Cargo.toml": "1.0.1"
	},
	"groups": {
		"sdk": "1.1.0"
	}
}
```

### It writes nothing

`monochange next` is read-only. It does not create `release.json`, does not write the prepared-release cache under `.monochange/local/`, and does not modify manifests, changelogs, or changesets. Running it always leaves a clean working tree, so it is safe in a pre-commit check, a shell prompt, or a CI job that only reports.

The one exception is reuse: if a valid prepared-release artifact already exists and still matches the workspace, the command reports that artifact instead of recomputing. A stale artifact is ignored and the plan is recomputed from changesets.

### When there are no changesets

An empty `.changeset` directory is a normal state, not an error. The command reports it and exits successfully:

```text
no package or group versions were planned
```

### Relationship to other commands

| Command                                     | Reports                                                       |
| ------------------------------------------- | ------------------------------------------------------------- |
| `monochange next`                           | Planned group and package versions only                       |
| `monochange versions list`                  | The _current_ versions recorded in the workspace              |
| `monochange step prepare-release --dry-run` | Planned versions plus changelog and release-artifact previews |

`monochange next` is the read-only alias for `monochange step display-versions`; `monochange next-versions` also resolves there.

Use `monochange versions list` when you want the versions that exist today, and `monochange next` when you want the versions that will exist after the next release.

## Publish subcommands

`monochange publish` groups the built-in publishing steps behind short subcommands. Each runs the same step as its `monochange step *` equivalent, with identical inputs and output formats.

| Command                          | Runs                                  |
| -------------------------------- | ------------------------------------- |
| `monochange publish packages`    | `monochange step publish-packages`    |
| `monochange publish readiness`   | `monochange step publish-readiness`   |
| `monochange publish placeholder` | `monochange step placeholder-publish` |

```bash
monochange publish readiness --from HEAD --output readiness.json
monochange publish packages --output publish.json
monochange publish placeholder --format json
```

The `monochange step *` forms remain supported and behave identically; the grouped commands exist for readability and tab completion. Any `[cli.*]` workflow you define in `monochange.toml` is unaffected, because config-defined commands run under `monochange run <name>`.

Typical first-time registry bootstrap:

```bash
monochange publish readiness --from HEAD --output readiness.json
monochange publish placeholder
monochange publish readiness --from HEAD --output readiness.json
monochange publish packages --output publish.json
```
