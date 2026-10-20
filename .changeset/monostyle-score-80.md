---
monochange: refactor
monochange_config: refactor
---

# Split the CLI dispatcher and changelog validation into focused helpers

`dispatch_cli_args` held every config-free fast path, configuration loading, clap error handling, `--jq` validation, and all subcommand arms in one function, and monostyle scored it at a cognitive complexity of 364, the largest complexity penalty in the repository. It is now a short sequence of calls: `render_config_free_request` answers version and built-in help before the workspace loads, `load_configuration_with_progress` reports the loading phase, `render_clap_parse_failure` turns parse errors into help or diagnostics, `run_cli_subcommand` dispatches the parsed subcommand to one helper per command, and `finish_cli_output` applies `--quiet`, `--jq`, and Markdown output once. Branches that could never run are removed: the second root-help checks, the post-parse `--snapshot` flag and `help` subcommand handling (both are always answered before parsing), and the usage errors for `step`, `run`, `change`, `api`, and `changeset`, whose nested subcommands clap already requires. The per-command `--quiet` checks collapse into the final step except for `skill`, which writes topic content directly. The now-unused `command_path_from_matches` helper is deleted.

`validate_changelog_configuration` in `monochange_config` is split the same way, into template, type, output, output-target, and excluded-type validators that keep the original checks, error messages, and order.

Behavior is unchanged. The repository's monostyle complexity score rises from 79.5 to 80.8, and `lint:monostyle` now fails below 80 for readability or complexity and runs in the CI `lint` job.
