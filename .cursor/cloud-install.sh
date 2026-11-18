#!/usr/bin/env bash
# Idempotent bootstrap for a Cursor cloud agent.
#
# Installs Nix (flakes and nix-command) and devenv, trusts the devenv Cachix
# binary cache, and links both tools into /usr/local/bin so login and
# non-login shells can find them. `devenv shell -- true` then materialises
# the pinned shell. Cargo binaries, the pnpm lockfile, and the crates.io
# index follow, matching .github/actions/devenv.
#
# macOS-only website builds (iOS Simulator) and a local Flutter SDK are not
# installed here. Android, iOS, and Flutter checks stay on their CI jobs.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

DEVENV_CACHE_URL="https://devenv.cachix.org"
DEVENV_CACHE_KEY="devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw="

if ! command -v nix >/dev/null 2>&1; then
	curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix |
		sh -s -- install linux --no-confirm --init none
fi

if [ -f /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh ]; then
	# shellcheck disable=SC1091
	. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
fi

if ! command -v nix >/dev/null 2>&1 && [ -x /nix/var/nix/profiles/default/bin/nix ]; then
	export PATH="/nix/var/nix/profiles/default/bin:${PATH}"
fi

if [ -d "${HOME}/.nix-profile/bin" ]; then
	export PATH="${HOME}/.nix-profile/bin:${PATH}"
fi

nix_config_has() {
	nix config show 2>/dev/null | grep -F -q -- "$1"
}

append_config_line() {
	local file="$1"
	local line="$2"
	if [ -f "$file" ] && grep -qxF -- "$line" "$file"; then
		return 0
	fi
	mkdir -p "$(dirname "$file")"
	printf '%s\n' "$line" >>"$file"
}

append_system_config_line() {
	local file="$1"
	local line="$2"
	if [ -f "$file" ] && grep -qxF -- "$line" "$file"; then
		return 0
	fi
	if command -v sudo >/dev/null 2>&1; then
		printf '%s\n' "$line" | sudo tee -a "$file" >/dev/null
	else
		printf '%s\n' "$line" >>"$file"
	fi
}

# Determinate Nix keeps local edits in nix.custom.conf. Other installs read
# the user nix.conf. Both are consulted for non-login shells.
user_nix_conf="${XDG_CONFIG_HOME:-${HOME}/.config}/nix/nix.conf"
system_nix_conf="/etc/nix/nix.custom.conf"
if [ ! -e "$system_nix_conf" ] && [ -e /etc/nix/nix.conf ]; then
	system_nix_conf="/etc/nix/nix.conf"
fi

ensure_nix_setting() {
	local line="$1"
	append_config_line "$user_nix_conf" "$line"
	if [ -e "$(dirname "$system_nix_conf")" ]; then
		append_system_config_line "$system_nix_conf" "$line"
	fi
}

if ! nix_config_has "nix-command" || ! nix_config_has "flakes"; then
	ensure_nix_setting "extra-experimental-features = nix-command flakes"
fi

if ! nix_config_has "devenv.cachix.org"; then
	if ! command -v cachix >/dev/null 2>&1; then
		nix profile install --accept-flake-config nixpkgs#cachix
	fi
	# Non-interactive: a prompt must not block the cloud-agent install.
	if ! cachix use devenv </dev/null; then
		ensure_nix_setting "extra-substituters = ${DEVENV_CACHE_URL}"
		ensure_nix_setting "extra-trusted-public-keys = ${DEVENV_CACHE_KEY}"
	fi
fi

if ! nix_config_has "devenv.cachix.org"; then
	ensure_nix_setting "extra-substituters = ${DEVENV_CACHE_URL}"
	ensure_nix_setting "extra-trusted-public-keys = ${DEVENV_CACHE_KEY}"
fi

if ! command -v devenv >/dev/null 2>&1; then
	nix profile install --accept-flake-config nixpkgs#devenv
fi

link_tool() {
	local name="$1"
	local target
	target="$(command -v "$name")"
	if [ ! -x /usr/local/bin/"$name" ] || [ "$(readlink -f /usr/local/bin/"$name" || true)" != "$(readlink -f "$target")" ]; then
		if command -v sudo >/dev/null 2>&1; then
			sudo ln -sfn "$target" /usr/local/bin/"$name"
		else
			ln -sfn "$target" /usr/local/bin/"$name"
		fi
	fi
}

link_tool nix
link_tool devenv

retry() {
	local attempt
	for attempt in 1 2 3; do
		if "$@"; then
			return 0
		fi
		if [ "$attempt" -eq 3 ]; then
			echo "command failed after ${attempt} attempts: $*" >&2
			return 1
		fi
		sleep $((attempt * 10))
	done
}

# Download the pinned shell closure from the binary cache before project deps.
retry devenv shell -- true

# cargo-run-bin and pnpm are the pieces CI installs before lint and test.
retry devenv shell -- install:cargo:bin
retry devenv shell -- pnpm install --frozen-lockfile
# The pinned nightly and the crates.io index, so the first agent build is not
# also a toolchain download. `cargo fetch` does not compile the workspace.
retry devenv shell -- rustup show
retry devenv shell -- cargo fetch --locked
