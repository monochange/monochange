#!/usr/bin/env bash
# Run official leptoskit component lints over every leptoskit.toml in the repo.
# Uses leptoskit_cli (`leptoskit lint`); see https://leptoskit.dev/docs/guide/23-linting.html
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

resolve_leptoskit() {
	# Prefer cargo-run-bin's install, which is pinned to the upstream Git
	# revision in [workspace.metadata.bin]. A bare `leptoskit` on PATH may be an
	# older crates.io release, so it is never selected implicitly.
	local candidate
	candidate="$(find "$root/.bin" -type f -name leptoskit 2>/dev/null | sort | tail -n 1 || true)"

	if [[ -n "${candidate}" && -x "${candidate}" ]]; then
		printf '%s\n' "${candidate}"

		return 0
	fi

	if command -v cargo >/dev/null 2>&1; then
		cargo bin --install >/dev/null
	fi

	candidate="$(find "$root/.bin" -type f -name leptoskit 2>/dev/null | sort | tail -n 1 || true)"

	if [[ -n "${candidate}" && -x "${candidate}" ]]; then
		printf '%s\n' "${candidate}"

		return 0
	fi

	echo "error: leptoskit CLI not found in .bin. Install the revision pinned in" >&2
	echo "  [workspace.metadata.bin] with:" >&2
	echo "  cargo bin --install" >&2
	echo "  # or: cargo install --git https://github.com/leptoskit/leptoskit --rev <rev> --locked leptoskit_cli" >&2

	return 1
}

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

leptoskit_bin="$(resolve_leptoskit)"
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
