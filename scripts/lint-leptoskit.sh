#!/usr/bin/env bash
# Run official leptoskit component lints over every leptoskit.toml in the repo.
# Uses leptoskit_cli (`leptoskit lint`); see https://leptoskit.dev/docs/guide/23-linting.html
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# The pinned CLI resolution (metadata read, lockfile source check, cache
# provenance, forced Git install) lives in one shared implementation that the
# build, migrate, and workspace commands also use.
mapfile -t configs < <(
	find "$root" \
		\( \
		-name target -o \
		-name .git -o \
		-name .devenv -o \
		-name .bin -o \
		-name node_modules -o \
		-name worktrees -o \
		-name fixtures -o \
		-name examples -o \
		-name evals \
		\) -prune -o \
		-type f -name leptoskit.toml -print | sort
)

if [[ ${#configs[@]} -eq 0 ]]; then
	echo "error: no leptoskit.toml found under $root" >&2
	exit 1
fi

leptoskit_bin="$("$root/scripts/leptoskit-cli.sh")"
status=0

for config in "${configs[@]}"; do
	project_dir="$(dirname "$config")"
	rel="${project_dir#"$root"/}"
	echo "Currently running: leptoskit lint ($rel)"
	# --no-clippy: workspace lint:clippy already denies Rust warnings.
	# --deny-warnings: treat leptoskit rule warnings as failures in CI.
	if ! (
		cd "$project_dir"
		"$leptoskit_bin" lint --deny-warnings --no-clippy
	); then
		status=1
	fi
done

exit "$status"
