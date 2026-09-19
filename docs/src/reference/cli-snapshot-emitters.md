# CLI snapshot emitters

A CLI snapshot is a normalized JSON document describing a tool's command surface: command paths, options, positionals, and parser behavior. monochange diffs that document against a committed baseline to classify command-surface breaks during `monochange change classify`.

[Package CLI registration](package-cli-registration.md) covers registering a binary. This page covers the other half: making a CLI actually print a snapshot document. The published schema at <https://monochange.github.io/monochange/schemas/command-snapshot.schema.json> is the contract, and the [schema reference](schemas.md) lists every hosted URL.

## The contract in one example

Every snapshot is one JSON document on stdout:

```json
{
	"schema_version": "0.1",
	"kind": "cli-surface",
	"tool": { "name": "demo", "version": "1.0.0" },
	"provenance": { "extractor": "commander", "confidence": "high" },
	"standard_entrypoints": {
		"help": { "flags": ["--help", "-h"] },
		"version": { "flags": ["--version", "-V"] },
		"snapshot": {}
	},
	"commands": [
		{
			"path": ["check"],
			"hidden": false,
			"max_bump": "major",
			"summary": "Check things",
			"parser": {
				"flags_are_posix_noncompliant": false,
				"options_must_precede_arguments": false,
				"option_arg_separators": [" ", "="]
			},
			"options": [
				{
					"names": ["--format"],
					"canonical_name": "--format",
					"hidden": false,
					"global": false,
					"summary": "Output format",
					"value": {
						"kind": "string",
						"required": false,
						"repeatable": false,
						"variadic": false
					}
				}
			]
		}
	]
}
```

Five fields are required: `schema_version`, `kind`, `tool`, `provenance`, and `standard_entrypoints`. Everything else may be omitted when empty, and the validator rejects unknown fields so typos fail loudly instead of being ignored.

Two values are fixed by the contract rather than chosen by you:

| Field            | Value           | Why                                                                      |
| ---------------- | --------------- | ------------------------------------------------------------------------ |
| `kind`           | `"cli-surface"` | Discriminator for snapshot documents.                                    |
| `schema_version` | `"0.1"`         | Must match the schema version monochange supports, or the capture fails. |

Read the current `schema_version` from the schema asset you pin against rather than hardcoding it forever; see [Version policy](#version-policy).

## What each field means

`tool` identifies the binary. `version` may be `null`, but supplying it makes snapshots easier to audit.

`provenance` records how the snapshot was produced. `extractor` is a free-form label such as `clap`, `commander`, `argparse`, or `help-text`. `confidence` is one of `high`, `medium`, or `low` and tells reviewers how much to trust the extraction:

- `high` for structured metadata read from a command framework's own definitions.
- `medium` for partial introspection, for example commands discovered but option types guessed.
- `low` for snapshots inferred from `--help` prose, where types, defaults, and parser behavior are not reliably recoverable.

Confidence is advisory metadata today; it does not currently change classification severity. Set it honestly so the record shows how the snapshot was derived.

`standard_entrypoints` normalizes help, version, and snapshot discovery across spelling variants, so `--help` and a `help` subcommand compare as the same capability. Each entrypoint takes `commands` (a list of command paths, each itself a list of segments) and `flags`. Omit what the tool does not support. It is recorded for documentation and agent discovery; the current diff does not compare it.

`commands` is a flat list of command nodes. Nested subcommands use the full path: a `get` command under `config` is one node with `"path": ["config", "get"]`, optionally also mirrored as a nested child. The diff keys on the path, so nested children are flattened and a node in `commands` and the same path nested under its parent are the same command. Always emit the complete path.

Note that `global_options` is recorded but not diffed, so put options that participate in a command's contract on that command's `options` list.

`max_bump` caps the release impact of changes at or below a command path. It defaults to `major`, which is the safe assumption for a public command. Lower it only for commands you deliberately treat as unstable.

`options` and `positionals` describe accepted arguments:

| Field               | Meaning                                                    |
| ------------------- | ---------------------------------------------------------- |
| `names`             | Every accepted spelling, for example `["-f", "--format"]`. |
| `canonical_name`    | The primary spelling used in findings.                     |
| `hidden`            | Whether the argument is hidden from help output.           |
| `global`            | Whether the option is accepted by subcommands too.         |
| `value.kind`        | One of `flag`, `string`, `enum`, or `counter`.             |
| `value.required`    | Whether the argument must be supplied.                     |
| `value.repeatable`  | Whether the option may be passed more than once.           |
| `value.variadic`    | Whether a positional accepts multiple values.              |
| `value.enum_values` | Accepted values when `kind` is `enum`.                     |
| `value.default`     | The default value as a string, when one exists.            |

`parser` captures invocation rules that affect compatibility. Use `[" ", "="]` for `option_arg_separators` when both `--flag value` and `--flag=value` work. Set `options_must_precede_arguments` when the parser stops recognizing options after the first positional, and `flags_are_posix_noncompliant` for parsers that accept combined short flags differently from POSIX.

## Which fields actually drive classification

Getting these right matters more than completeness, because they are what produce findings:

- **Command paths.** A missing path reads as a removed command (`major`).
- **`options[].names`.** Dropping a spelling reads as a removed option; adding one reads as additive.
- **`value.kind` and `value.enum_values`.** Narrowing (`string` to `enum`, or removing an enum value) proposes `major`; widening proposes `minor`.
- **`value.required`.** An optional argument becoming required is breaking.
- **`max_bump`.** Caps the severity proposed for that command.

Descriptions (`summary`, `description`) only ever produce compatible patches, so you can refine wording freely. Conversely, omitting a real option is not harmless: the baseline records it, so the next capture looks like a removal.

## Rust: emit from clap

The `monochange_snapshot` crate ships a clap extractor, so no hand-written mapping is needed. Add the crate and print the snapshot in a subcommand:

```rust
use clap::Command;
use monochange_snapshot::snapshot_from_clap;

let command = build_cli();
let snapshot = snapshot_from_clap(&command);
println!("{}", snapshot.to_json()?);
```

`to_json` renders pretty-printed JSON with a trailing newline. For a fuller extractor with control over provenance:

```rust
use monochange_snapshot::ClapCommandSurfaceExtractor;
use monochange_snapshot::CommandSurfaceExtractor;

let extractor = ClapCommandSurfaceExtractor::new(&build_cli());
let snapshot = extractor.extract();
```

This produces a `high`-confidence snapshot labelled with the `clap` extractor. If your binary already uses monochange, `monochange snapshot --view index` prints an equivalent document and you do not need a subcommand of your own.

## TypeScript and JavaScript

Node CLIs built on [commander](https://github.com/tj/commander.js) expose their definitions at runtime, so an emitter can read them directly. This works because commander keeps parsed option metadata on each command.

```js
import { Command } from "commander";

function optionEntry(option) {
	const names = option.flags
		.split(/[ ,|]+/)
		.filter((token) => token.startsWith("-"));
	const takesValue = option.required || option.optional;
	const canonical =
		option.long ?? names.find((name) => name.startsWith("--")) ?? names[0];
	return {
		names,
		canonical_name: canonical,
		hidden: Boolean(option.hidden),
		global: false,
		...(option.description ? { summary: option.description } : {}),
		value: {
			kind: takesValue ? "string" : "flag",
			required: false,
			repeatable: Boolean(option.variadic),
			variadic: Boolean(option.variadic),
			...(option.defaultValue === undefined
				? {}
				: { default: String(option.defaultValue) }),
		},
	};
}

function commandNode(command, parentPath) {
	const path = [...parentPath, command.name()];
	return {
		path,
		hidden: false,
		max_bump: "major",
		...(command.summary() ? { summary: command.summary() } : {}),
		parser: {
			flags_are_posix_noncompliant: false,
			options_must_precede_arguments: false,
			option_arg_separators: [" ", "="],
		},
		options: command.options.map(optionEntry),
		positionals: [],
		commands: command.commands.map((child) => commandNode(child, path)),
	};
}

export function emitSnapshot(
	program,
	{ extractor = "commander", confidence = "high" } = {},
) {
	return {
		schema_version: "0.1",
		kind: "cli-surface",
		tool: { name: program.name(), version: program.version() ?? null },
		provenance: { extractor, confidence },
		standard_entrypoints: {
			help: { flags: ["--help", "-h"] },
			version: { flags: ["--version", "-V"] },
			snapshot: {},
		},
		commands: program.commands.map((child) => commandNode(child, [])),
	};
}
```

Wire it into the program and write it to stdout:

```js
import { Command } from "commander";
import { emitSnapshot } from "./emit-snapshot.mjs";

const program = new Command("demo").version("1.0.0");
program
	.command("check")
	.description("Check things")
	.option("-f, --format <value>", "Output format", "text");

if (process.argv[2] === "snapshot") {
	process.stdout.write(`${JSON.stringify(emitSnapshot(program), null, 2)}\n`);
	process.exit(0);
}

program.parse();
```

Register it in `monochange.toml` with the CLI snapshot subcommand:

```toml
[package.demo]
path = "packages/demo"
cli = { name = "demo", snapshot = { command = "node dist/index.js snapshot", cwd = "packages/demo" } }
```

Point `snapshot` at the built entry point, since emitters that read a live program need the CLI to load. Add `shell = true` only when the command needs shell features such as pipes or environment expansion.

Other Node frameworks (yargs, oclif, clipanion) expose comparable command metadata. Map their structures into the same shape, or fall back to the help-text approach below. yargs in particular keeps `.getOptions()` per command, and oclif exposes a manifest via `oclif manifest`.

## Python

For `argparse`, read each parser's `_actions`. Private attributes are acceptable here because the emitter is a development tool pinned to a known CLI, but assert on the fields you depend on so a Python upgrade fails loudly rather than silently emitting a wrong snapshot.

```python
"""Emit a normalized command surface snapshot for an argparse CLI."""
import argparse


def option_entry(action):
    names = list(action.option_strings)
    if not names:
        return None
    takes_value = action.nargs != 0
    canonical = next((n for n in names if n.startswith("--")), names[0])
    value = {
        "kind": "string" if takes_value else "flag",
        "required": bool(action.required),
        "repeatable": action.nargs in ("*", "+"),
        "variadic": action.nargs in ("*", "+"),
    }
    if action.default is not None and takes_value:
        value["default"] = str(action.default)
    if action.choices is not None:
        value["kind"] = "enum"
        value["enum_values"] = [str(c) for c in action.choices]
    entry = {
        "names": names,
        "canonical_name": canonical,
        "hidden": False,
        "global": False,
        "value": value,
    }
    if action.help:
        entry["summary"] = action.help
    return entry


def command_node(parser, parent_path):
    path = parent_path + [parser.prog.split()[-1]]
    options = [e for e in (option_entry(a) for a in parser._actions) if e]
    options = [o for o in options if o["canonical_name"] not in ("--help", "-h")]
    positionals = [
        {
            "name": a.dest,
            "hidden": False,
            "value": {
                "kind": "string",
                "required": a.required,
                "repeatable": a.nargs in ("*", "+"),
                "variadic": a.nargs in ("*", "+"),
            },
        }
        for a in parser._actions
        if not a.option_strings and a.dest != "help"
    ]
    for positional in positionals:
        action = next(a for a in parser._actions if a.dest == positional["name"])
        if action.help:
            positional["summary"] = action.help
    return {
        "path": path,
        "hidden": False,
        "max_bump": "major",
        "parser": {
            "flags_are_posix_noncompliant": False,
            "options_must_precede_arguments": False,
            "option_arg_separators": [" ", "="],
        },
        "options": options,
        "positionals": positionals,
        "commands": [],
    }


def emit_snapshot(name, version, commands, extractor="argparse", confidence="high"):
    return {
        "schema_version": "0.1",
        "kind": "cli-surface",
        "tool": {"name": name, "version": version},
        "provenance": {"extractor": extractor, "confidence": confidence},
        "standard_entrypoints": {
            "help": {"flags": ["--help", "-h"]},
            "version": {"flags": ["--version"]},
            "snapshot": {},
        },
        "commands": commands,
    }
```

Walk the subparser tree to build complete paths, then print the document:

```python
import argparse
import json

from emit_snapshot import command_node, emit_snapshot

parser = argparse.ArgumentParser(prog="demo", description="Demo tool")
sub = parser.add_subparsers(dest="command")
check = sub.add_parser("check", help="Check things")
check.add_argument("-f", "--format", default="text", help="Output format")

commands = []
for sub_parser in parser._subparsers._group_actions[0].choices.values():
    node = command_node(sub_parser, [])
    if sub_parser._subparsers:
        children = sub_parser._subparsers._group_actions[0].choices
        node["commands"] = [command_node(child, node["path"]) for child in children.values()]
    commands.append(node)

print(json.dumps(emit_snapshot("demo", "1.0.0", commands), indent=2))
```

Both Click and Typer expose structured command trees: Click commands carry `.params` with `.opts`, `.is_flag`, and `.multiple`, and Typer builds on Click. Fire and docopt are weaker fits because they derive interfaces from signatures or docstrings, so either map their introspection output or mark the result `medium`/`low` confidence.

## Go

Go CLIs commonly build commands imperatively, so the most reliable emitter mirrors the command definitions you already declare. With [cobra](https://github.com/spf13/cobra), read `cmd.Commands()`, `cmd.Flags()`, and `cmd.NonInheritedFlags()`; with the standard library `flag` package or `pflag`, iterate the flag set.

Define the snapshot as plain structs so the JSON shape is checked at compile time:

```go
package main

import (
	"encoding/json"
	"os"
)

type Value struct {
	Kind       string   `json:"kind"`
	Required   bool     `json:"required"`
	Repeatable bool     `json:"repeatable"`
	Variadic   bool     `json:"variadic"`
	EnumValues []string `json:"enum_values,omitempty"`
	Default    *string  `json:"default,omitempty"`
}

type Option struct {
	Names         []string `json:"names"`
	CanonicalName string   `json:"canonical_name"`
	Hidden        bool     `json:"hidden"`
	Global        bool     `json:"global"`
	Summary       string   `json:"summary,omitempty"`
	Value         Value    `json:"value"`
}

type Positional struct {
	Name    string `json:"name"`
	Hidden  bool   `json:"hidden"`
	Summary string `json:"summary,omitempty"`
	Value   Value  `json:"value"`
}

type Parser struct {
	FlagsArePosixNoncompliant   bool     `json:"flags_are_posix_noncompliant"`
	OptionsMustPrecedeArguments bool     `json:"options_must_precede_arguments"`
	OptionArgSeparators         []string `json:"option_arg_separators"`
}

type Command struct {
	Path        []string     `json:"path"`
	Hidden      bool         `json:"hidden"`
	MaxBump     string       `json:"max_bump"`
	Summary     string       `json:"summary,omitempty"`
	Parser      Parser       `json:"parser"`
	Options     []Option     `json:"options,omitempty"`
	Positionals []Positional `json:"positionals,omitempty"`
	Commands    []Command    `json:"commands,omitempty"`
}

type Entrypoint struct {
	Commands [][]string `json:"commands,omitempty"`
	Flags    []string   `json:"flags,omitempty"`
}

type Tool struct {
	Name    string  `json:"name"`
	Version *string `json:"version"`
}

type Provenance struct {
	Extractor  string `json:"extractor"`
	Confidence string `json:"confidence"`
}

type StandardEntrypoints struct {
	Help     Entrypoint `json:"help"`
	Version  Entrypoint `json:"version"`
	Snapshot Entrypoint `json:"snapshot"`
}

type Snapshot struct {
	SchemaVersion       string              `json:"schema_version"`
	Kind                string              `json:"kind"`
	Tool                Tool                `json:"tool"`
	Provenance          Provenance          `json:"provenance"`
	StandardEntrypoints StandardEntrypoints `json:"standard_entrypoints"`
	Commands            []Command           `json:"commands,omitempty"`
}
```

Build the tree and encode it, setting `version := "1.0.0"` and `&version`:

```go
func emit(name string, version *string, commands []Command) error {
	snapshot := Snapshot{
		SchemaVersion: "0.1",
		Kind:          "cli-surface",
		Tool:          Tool{Name: name, Version: version},
		Provenance:    Provenance{Extractor: "cobra", Confidence: "high"},
		StandardEntrypoints: StandardEntrypoints{
			Help:     Entrypoint{Flags: []string{"--help", "-h"}},
			Version:  Entrypoint{Flags: []string{"--version"}},
			Snapshot: Entrypoint{},
		},
		Commands: commands,
	}
	encoder := json.NewEncoder(os.Stdout)
	encoder.SetIndent("", "  ")
	return encoder.Encode(snapshot)
}
```

Note that `version` must be a pointer (or omitted) so the field serializes as either a string or `null`. A plain `string` always emits `""`, which the schema accepts but misrepresents an unknown version.

Go has no reflection-based option introspection comparable to clap's, because flags live in a user-defined struct. Reading the `flag.FlagSet` or cobra command values you construct is the reliable path.

## Dart and Flutter

The `args` package exposes an `ArgParser` whose subcommands and options are enumerable at runtime. Map `ArgParser.commands` recursively, using the same path convention. Build the document with `dart:convert`:

```dart
import 'dart:convert';

Map<String, Object?> emitSnapshot({
  required String name,
  String? version,
  required List<Map<String, Object?>> commands,
}) {
  return {
    'schema_version': '0.1',
    'kind': 'cli-surface',
    'tool': {'name': name, 'version': version},
    'provenance': {'extractor': 'dart/args', 'confidence': 'high'},
    'standard_entrypoints': {
      'help': {'flags': ['--help', '-h']},
      'version': {'flags': ['--version']},
      'snapshot': <String>[],
    },
    'commands': commands,
  };
}

void main() {
  final snapshot = emitSnapshot(name: 'demo', version: '1.0.0', commands: const []);
  print(const JsonEncoder.withIndent('  ').convert(snapshot));
}
```

Read each `ArgParser`'s `options` for flag metadata (`isFlag`, `abbr`, `defaultsTo`) and `commands` for nested parsers. For Flutter tools, the host CLI is usually a Dart entry point, so the same approach applies.

## Help-text inference

When no structured metadata exists, parse `--help` output. Treat this as a last resort: it recovers command paths and option names reasonably well but cannot reliably determine value kinds, defaults, or parser behavior.

```bash
mycli --help > help.txt
```

Mark these snapshots `"confidence": "low"` so the record shows how the document was derived. Be aware that `provenance.confidence` is currently recorded metadata only: `monochange change classify` reports command-surface findings at high confidence regardless of it, so a help-text snapshot carries the same enforcement weight as a clap-extracted one. That is the main reason to prefer structured extraction, because a wrong guess about a value kind becomes a real `major` finding rather than a hedged one.

## Recommended pattern: a hidden or explicit snapshot subcommand

Give the CLI its own snapshot entry point rather than a separate script, because the subcommand:

- keeps the emitter next to the definitions it reads, so it cannot drift;
- runs in the same environment as the CLI, including built artifacts;
- avoids duplicating command definitions in a second language.

The name `snapshot` is a good choice because it reads clearly and monochange already reserves it for this purpose. Reserving it is only strictly required inside `monochange.toml`: `[cli.*]` workflow names cannot shadow monochange's built-in commands. A hidden `snapshot` subcommand in your own CLI is fine; discoverability is not required.

```js
if (process.argv[2] === "snapshot") {
	process.stdout.write(`${JSON.stringify(emitSnapshot(program), null, 2)}\n`);
	process.exit(0);
}
```

## Validate before you register

Validate the emitted document against the published schema before wiring it into `monochange.toml`, so schema drift is a local error rather than a confusing classification warning. Any draft 2020-12 validator works:

```sh
# Node
node -e "
const Ajv = require('ajv/dist/2020');
const schema = require('./command-snapshot.schema.json');
const doc = require('./snapshot.json');
const validate = new Ajv({ strict: false }).compile(schema);
if (!validate(doc)) { console.error(validate.errors); process.exit(1); }
console.log('valid');
"
```

```sh
# Python
python3 -c "
import json, jsonschema
schema = json.load(open('command-snapshot.schema.json'))
doc = json.load(open('snapshot.json'))
jsonschema.validate(doc, schema)
print('valid')
"
```

Because the schema sets `additionalProperties: false`, an unexpected field is a validation failure. That is intentional: it catches misspelled field names that would otherwise silently drop data from the classification.

## Should you share a helper package?

Reusing one emitter across repositories is tempting, but the tradeoff is different from a typical utility library, because the emitter has to run inside the CLI process to read the live command tree.

That makes any adapter a **runtime** dependency of the CLI, shipped in the published artifact even though it only matters at release time. Weigh that against what an adapter saves: the mapping code above is roughly 60 lines, and each command framework needs its own version because commander, yargs, and oclif expose different introspection APIs. An adapter package therefore costs a runtime dependency plus a per-framework maintenance surface, and repays it only if you maintain several Node CLIs that share one framework.

A narrower helper avoids the runtime cost: ship a schema-driven **validator** rather than an emitter. Validation runs in CI on an already-emitted file, so it can be a dev dependency, covers every framework at once, and catches the failure that actually matters, which is a snapshot that does not match the contract. Combined with the per-language examples on this page, that covers most of the value without imposing on the CLI's runtime dependencies.

If a shared emitter is still worth it for your organization, keep it a thin wrapper that emits the document shape and let callers pass in already-extracted command data. That keeps framework-specific introspection out of the shared package and lets it stay a dev dependency when the CLI structure is generated at build time rather than read at runtime.

## Version policy

Snapshot documents carry their own `schema_version`, derived from the `monochange_snapshot` crate version rather than the configuration schema version. The two are independent:

- `monochange.toml` uses one schema version (currently `0.6`).
- Command snapshots use another (currently `0.1`).

A snapshot captured with a version monochange does not support is rejected and reported as a capture warning, not a silent skip. Pin your emitter to the version that matches the monochange release you run in CI, and expect a version bump to require regenerating baselines.

## Troubleshooting

`cli snapshot for <name> was not compared: no committed baseline` means no baseline file exists yet. Capture one with `monochange snapshot --package <id> --save`.

`stale_baseline` means the committed baseline uses a different `schema_version` than this monochange build supports. Regenerate the baseline.

`failed` means the snapshot command returned a non-zero exit, printed unparsable JSON, or emitted a document that does not match the schema. Run the configured command by hand with `monochange snapshot --package <id>`, which prints the output, and validate it against the schema.

A capture that needs a build unavailable in a given environment can be skipped with `--skip-cli-snapshots` or `MONOCHANGE_SKIP_CLI_SNAPSHOTS=1`. Prefer fixing the environment: skipped comparisons mean command-surface breaks are reported as unclassified changes again.

## Related pages

- [Package CLI registration](package-cli-registration.md): registering a binary and committing baselines.
- [Schema reference](schemas.md): hosted schema URLs and versioning.
- [Change classification](change-classification.md): how findings reach the pull request.
