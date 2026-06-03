# CLI surface snapshots

## Status

- Branch: `feat/cli-surface-plan`
- Worktree: `/Users/ifiokjr/.pi/agent/worktrees/root/root/Users/ifiokjr/Developer/projects/monochange/monochange/worktrees/feat-cli-surface-plan`
- State: planning only; do not implement until the schema and naming are agreed.

## Problem statement

monochange can classify source API changes, but it does not yet treat a command-line interface as a versioned API surface. CLI users and agents depend on command paths, aliases, flags, positional arguments, accepted values, parser behavior, output formats, structured output fields, error kinds, and help/discovery affordances. Breaking those contracts should be detectable and should feed the same release-impact and changeset-alignment policy as source API snapshots.

The desired outcome is a monochange-owned normalized CLI surface snapshot that can be generated from a CLI, stored as an inspectable fixture/snapshot, diffed across revisions, and classified into major/minor/patch/no-op recommendations.

## Naming decision

Use `monochange_snapshot` for the normalized snapshot crate/module.

Rationale:

- Avoids doubled terms like `cli_api`.
- Leaves room for more than CLI invocation snapshots over time: source API snapshots, CLI surface snapshots, output-contract snapshots, and possibly other compatibility surfaces.
- Matches monochange's existing direction: compare normalized, monochange-owned snapshots rather than adopting external formats as source of truth.
- Keeps the mental model simple for users and agents: snapshot current surface, diff snapshots, classify release impact.

## Research notes

### Fig / Amazon Q autocomplete

The strongest real-world reference is `withfig/autocomplete`, now tied to Amazon Q Developer CLI:

- Large adoption signal: roughly 25k GitHub stars from search result metadata.
- README advertises 400+ contributors.
- Repository contains around 1,484 TypeScript completion specs.
- Published package observed as `@withfig/autocomplete@2.692.3`, modified in 2025.
- It covers messy real-world CLIs such as `git`, `aws`, `docker`, and `npm`.

Fig's model is valuable because it survived real CLI weirdness. Its useful concepts:

- Recursive command tree: `Spec` / `Subcommand`.
- Command and option aliases via `name: string | string[]`.
- Options with `args`, `isRequired`, `isPersistent`, repeat/separator behavior, and dependencies/exclusions.
- Positionals with optional and variadic flags.
- Parser directives for non-POSIX behavior:
  - `flagsArePosixNoncompliant`
  - `optionsMustPrecedeArguments`
  - `optionArgSeparators`
- Version-specific diffs (`ArgDiff`, `OptionDiff`, `SubcommandDiff`, `VersionDiffMap`) as evidence that CLI surface drift was a real problem for a mature completion ecosystem.

Fig is only prior art for representing invocation structure. `monochange_snapshot` is not a completion system. Dynamic completions, shell completion generation, ranking, icons, and suggestion generators are non-goals unless they fall out almost for free from the normalized schema.

The primary consumer is an agent or CI policy engine that needs to quickly inspect what a tool can do, compare current and previous snapshots, and classify compatibility impact. Descriptions and examples matter for agents, but description-only changes are not breaking changes.

### Other formats, lower confidence

- `CLI Schema` and `OpenCLI` are useful design references but currently look immature/draft-like compared with Fig's adoption.
- `clispec` is useful for agent-facing principles: structured output, schema introspection, stderr/stdout separation, non-interactivity, idempotency, and bounded output.
- POSIX utility conventions remain useful vocabulary for options, option arguments, and operands.

Decision: do not adopt any external schema as the compatibility source of truth. Use Fig as the strongest practical reference, clispec as agent-output guidance, and keep a monochange-owned normalized model.

## Proposed normalized snapshot model

A snapshot should be compact, stable, sorted, and easy to grep. It should normalize common CLI affordances so semantically equivalent surfaces compare consistently.

The model should be framework-neutral. `clap` should be the first high-quality extractor because monochange uses it and it exposes structured command metadata, but the normalized snapshot must not be clap-shaped. Future extractors should be able to target the same schema from TypeScript/JavaScript CLI builders, Python CLI frameworks, generated shell completions, static Fig-like specs, hand-authored declarations, or command help introspection.

Extractor confidence should be explicit. A snapshot generated from clap metadata can be `high` confidence for commands/options/positionals. A snapshot inferred from `--help` text should be marked lower confidence and should avoid overclaiming types, parser behavior, defaults, or output contracts unless they are explicitly declared.

Sketch:

```json
{
	"schemaVersion": 1,
	"kind": "cli-surface",
	"tool": {
		"name": "mc",
		"version": "0.6.8"
	},
	"standardEntrypoints": {
		"help": {
			"commands": [["help"]],
			"flags": ["--help", "-h"]
		},
		"version": {
			"commands": [["version"]],
			"flags": ["--version", "-V"]
		},
		"schema": {
			"commands": [["schema"], ["snapshot"]],
			"flags": ["--schema", "--snapshot"]
		}
	},
	"globalOptions": [],
	"commands": [
		{
			"path": ["step:affected-packages"],
			"aliases": [],
			"hidden": false,
			"stability": "stable",
			"summary": "Evaluate packages affected by changed paths and changesets",
			"description": "Returns package ids affected by changed paths and checks changeset coverage policy.",
			"parser": {
				"flagsArePosixNoncompliant": false,
				"optionsMustPrecedeArguments": false,
				"optionArgSeparators": [" ", "="]
			},
			"intent": {
				"mutating": false,
				"destructive": false,
				"idempotent": true,
				"requiresAuth": false,
				"requiresNetwork": false
			},
			"options": [
				{
					"names": ["--from"],
					"canonicalName": "--from",
					"value": {
						"type": "string",
						"required": false,
						"repeatable": false,
						"variadic": false,
						"default": null,
						"enumValues": []
					}
				},
				{
					"names": ["--format"],
					"canonicalName": "--format",
					"value": {
						"type": "enum",
						"required": false,
						"enumValues": ["json", "text"]
					}
				}
			],
			"positionals": [],
			"outputs": [
				{
					"format": "json",
					"schemaRef": "affected-packages.output.schema.json",
					"stability": "stable"
				},
				{
					"format": "text",
					"stability": "human-readable"
				}
			],
			"errors": [
				{
					"kind": "config",
					"exitCode": 1
				}
			]
		}
	]
}
```

### Standard entrypoint normalization

Normalize common entrypoints rather than treating every spelling as an unrelated command:

- Help:
  - flags: `--help`, `-h`
  - commands: `help`, `help <command>`
- Version:
  - flags: `--version`, `-V`
  - commands: `version`
- Schema/snapshot/capabilities:
  - flags: `--schema`, `--snapshot`, possibly hidden metadata flags
  - commands: `schema`, `snapshot`, `__schema`

Compatibility policy should still notice removal of a supported spelling. Normalization is for comparison and agent discovery, not for ignoring compatibility breaks.

## Semantic classification policy draft

### Major / breaking

Invocation contract:

- Remove command or subcommand path.
- Remove command alias.
- Rename command without retaining old alias.
- Remove option/flag.
- Remove option alias or short flag.
- Change option value kind incompatibly (`boolean` to value-taking option, `string` to `integer`, `enum` to unrelated type, etc.).
- Make optional option or positional required.
- Add a new required positional to an existing command.
- Reorder positionals.
- Change optional positional to required.
- Change variadic positional to non-variadic.
- Remove repeatable support.
- Remove accepted enum value.
- Remove separator support such as `--flag=value`.
- Change parser behavior so previously valid invocations no longer parse.
- Remove persistent/global option availability from subcommands.
- Mark a previously stable command as experimental-only or hidden if discoverability is part of the contract.

Value compatibility:

- CLI values are string-oriented at the transport boundary. Compatibility should model accepted token shapes rather than programming-language types.
- Widening accepted values is additive, not breaking. Examples:
  - `string` -> `string | repeated string`
  - `string` -> `string | comma-separated-list`
  - `string` -> `string | enum-plus-custom-string`
  - `string` -> `string | bool-like-token` when the option still accepts previous string invocations
- Narrowing accepted values is breaking. Examples:
  - arbitrary `string` -> limited enum
  - repeated values -> single value only
  - `--flag value` no longer accepted because only `--flag=value` works
  - previous enum value removed
- Changing internal representation is not semantically relevant if all previously accepted invocations still parse and behave compatibly.

Output contract:

- Remove structured output format, especially `json`.
- Remove stable JSON output field.
- Change stable JSON output field type incompatibly.
- Change array/object cardinality or envelope shape incompatibly.
- Remove structured error kind.
- Change documented exit code semantics.
- Move data from stdout to stderr or mix diagnostics into structured stdout.

Behavior/risk contract:

- Command becomes mutating/destructive when previously read-only.
- Command starts requiring auth/network where it did not before.
- Command starts requiring interactive input in non-TTY contexts.

### Minor / additive

- Add command/subcommand.
- Add command alias.
- Add optional option.
- Add option alias.
- Add accepted enum value.
- Add optional positional after existing required positionals if parsing remains compatible.
- Add output format.
- Add optional JSON output field.
- Add structured error kind.
- Add examples, summaries, richer completion metadata.

### Patch / no recommendation

- Description/help wording changes.
- Example wording changes.
- Added or removed prose-only documentation, unless a command is intentionally marked as having stable help text.
- Human-readable text/table formatting changes when stable structured output is unchanged.
- Icon/display/priority/autocomplete ranking metadata changes.
- Reordering of fields in JSON objects where order is not semantic.
- Completion suggestion improvements that do not change accepted invocations.

## Agent-facing usage model

The snapshot should be generated as a JSON artifact that agents can inspect with grep/jq instead of traversing `--help` output command by command. For large CLIs, the artifact should be line-friendly and command-path searchable.

Descriptions are first-class because they help agents choose the right command and avoid unsupported invocations. However, descriptions can dominate context for large tools, so the viewer should support compact modes.

Possible layouts and views:

1. One complete snapshot file, stable sorted by command path.
2. Optional index file mapping command paths to per-command files.
3. Optional `mc snapshot show <path> --format json` later for focused retrieval.
4. Full view: includes descriptions, examples, output notes, and compatibility metadata.
5. Light view: omits descriptions/examples/prose and keeps only command paths, options, positionals, parser behavior, outputs, errors, stability, and semantic ids.
6. Index view: command paths plus short summaries only.

Initial preference: one complete snapshot plus a built-in renderer that can produce full/light/index views.

## Implementation phases once approved

- [x] Finalize naming: `monochange_snapshot`.
- [x] Finalize initial normalized schema fields for CLI invocation snapshots.
- [x] Decide first scope: monochange's own `mc` CLI via clap metadata.
- [x] Add internal data model crate/module.
- [x] Add extractor trait that produces the normalized snapshot plus confidence/provenance metadata.
- [x] Add first extractor from clap metadata.
- [x] Reserve room for future extractors: TypeScript/JavaScript CLI builders, Python frameworks, Fig-like specs, shell completion scripts, and help-text inference.
- [x] Add snapshot rendering modes: full, light/no-descriptions, and command index.
- [x] Add manual annotation data types for command output schemas where extractors cannot infer them.
- [x] Add snapshot rendering command: `mc snapshot` plus global `--snapshot`.
- [x] Add fixture-backed integration snapshots for `mc` surface output.
- [x] Add diff classifier for CLI surface snapshots.
- [x] Integrate classification into `mc change classify`.
- [x] Keep affected changeset policy enforcement deferred until CLI classification is trusted by fixture snapshots and a stored baseline format.

## Open questions

1. Name: should this be `monochange_surface`, `monochange_contract`, or something more command-specific?
2. Command spelling: should `step:affected-packages` be modeled as one path segment or normalized as `step affected-packages` with an alias back to the colon spelling?
3. Standard metadata command: should monochange expose `mc schema`, `mc snapshot`, `mc __schema`, or only an explicit command under `mc api`/`mc surface`?
4. Output schemas: should we derive with `schemars`, hand-author stable output contracts, or start with snapshot examples and graduate to schemas?
5. Stability tags: should every command default to stable, or should experimental/user-defined commands be marked separately?
6. User-defined `[cli.*]` commands: are they part of the package API surface, or only built-in binary/step commands?
7. External CLIs: should monochange eventually support snapshotting any binary, or only its own CLI and Rust/clap crates in the workspace?
8. TypeScript/JavaScript/Python extraction: do we prefer native adapters per framework, or a generic intermediate JSON declaration that those ecosystems can emit themselves?
9. Confidence policy: should low-confidence help-text snapshots participate in breaking-change enforcement, or only advisory reporting until manually annotated?

## Non-goals for the first implementation

- Do not build shell completion generation as part of the first implementation.
- Do not execute dynamic completion generators during classification.
- Do not adopt Fig, OpenCLI, CLI Schema, or clispec as the source-of-truth format.
- Do not attempt to semantically classify prose-only help output.
- Do not promise compatibility for human text output unless a command explicitly marks text output as stable.
- Do not classify shell-specific completion scripts as the API source of truth; they can be generated/exported later.

## Validation commands for future implementation

Use the repository's normal validation once code exists:

```nushell
devenv shell cargo fmt
devenv shell lint:clippy
devenv shell cargo test -p monochange --lib
devenv shell cargo test -p monochange_integration_tests --test api_classification
node scripts/check-patch-coverage.ts --repo-root $env.PWD --lcov target/coverage/lcov.info --base origin/main --head HEAD --target 100
```
