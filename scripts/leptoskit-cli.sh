#!/usr/bin/env bash
# Resolve the provenance-verified leptoskit CLI managed by
# [workspace.metadata.bin]. One implementation of the pin, shared by
# `lint:leptoskit`, `build:web`, `leptoskit migrate`, and `leptoskit workspace`
# commands: the lockfile check, cache lookup, and install all read the same
# definition and never trust a same-version registry binary.
#
# With no arguments, prints the verified binary path. With arguments, runs the
# binary. Install output and failures go to stderr.
#
#   leptoskit_bin="$(scripts/leptoskit-cli.sh)"
#   scripts/leptoskit-cli.sh migrate --check
#   scripts/leptoskit-cli.sh build --release --platform web --out target/site
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

package_name="leptoskit_cli"
git_url=""
git_rev=""
package_version=""
bin_name=""
metadata_locked="false"

fail() {
	printf 'error: %s\n' "$*" >&2

	exit 1
}

# Read the pinned source from [workspace.metadata.bin] so the lockfile check,
# cache lookup, and install all use one definition. The revision and version
# must not be copied into this script.
read_bin_metadata() {
	local metadata_json entry

	metadata_json="$(cargo metadata --no-deps --format-version 1)" ||
		fail "cargo metadata --no-deps failed; cannot read [workspace.metadata.bin]"

	entry="$(jq -c '.metadata.bin.leptoskit_cli // empty' <<<"${metadata_json}")"

	if [[ -z "${entry}" || "${entry}" == "null" ]]; then
		fail "leptoskit_cli is missing from [workspace.metadata.bin]"
	fi

	git_url="$(jq -r '.git // empty' <<<"${entry}")"
	git_rev="$(jq -r '.rev // empty' <<<"${entry}")"
	package_version="$(jq -r '.version // empty' <<<"${entry}")"
	bin_name="$(jq -r 'if (.bins | type) == "array" and (.bins | length) == 1 then .bins[0] else empty end' <<<"${entry}")"
	metadata_locked="$(jq -r '.locked // false' <<<"${entry}")"

	[[ -n "${git_url}" ]] || fail "leptoskit_cli metadata must set git"
	[[ "${git_rev}" =~ ^[0-9a-f]{40}$ ]] || fail "leptoskit_cli metadata must set a full 40-character rev (found '${git_rev}')"
	[[ -n "${package_version}" ]] || fail "leptoskit_cli metadata must set version"
	[[ -n "${bin_name}" ]] || fail "leptoskit_cli metadata must set exactly one entry in bins"
}

# Print name, version, and source for every package in a Cargo.lock. Cargo
# writes one [[package]] table per package, so a line-oriented parser is enough
# and avoids resolving the whole dependency graph for a provenance check.
lock_packages() {
	awk '
		function flush() {
			if (name != "") {
				print name "\t" version "\t" source
			}
		}
		/^\[\[package\]\]$/ {
			flush()
			name = ""
			version = ""
			source = ""
			next
		}
		/^name = "/ {
			line = $0
			sub(/^name = "/, "", line)
			sub(/"$/, "", line)
			name = line
			next
		}
		/^version = "/ {
			line = $0
			sub(/^version = "/, "", line)
			sub(/"$/, "", line)
			version = line
			next
		}
		/^source = "/ {
			line = $0
			sub(/^source = "/, "", line)
			sub(/"$/, "", line)
			source = line
			next
		}
		END {
			flush()
		}
	' "$1"
}

# Reject a workspace whose lockfile has leptoskit packages from a registry or a
# different revision. Checking every package, not the first match, catches a
# partial update that leaves one crate on the old source.
verify_lockfile_sources() {
	local lockfile package version source matched source_prefix source_suffix

	source_prefix="git+${git_url}"
	source_suffix="#${git_rev}"
	for lockfile in app/Cargo.lock app/native/Cargo.lock extension/Cargo.lock; do
		[[ -f "${lockfile}" ]] || fail "missing ${lockfile}; cannot verify the leptoskit Git revision"

		matched=0

		while IFS=$'\t' read -r package version source; do
			[[ "${package}" == leptoskit* ]] || continue

			matched=1

			if [[ "${source}" != "${source_prefix}${source_suffix}" && "${source}" != "${source_prefix}"?*"${source_suffix}" ]]; then
				fail "${lockfile} has ${package} ${version} from '${source:-<no source>}'; expected git ${git_url} at ${git_rev}"
			fi
		done < <(lock_packages "${lockfile}")

		[[ "${matched}" -eq 1 ]] || fail "${lockfile} has no leptoskit packages; expected git ${git_url} at ${git_rev}"
	done
}

# cargo-run-bin stores installs under .bin/rust-<toolchain>/<crate>/<version>
# and uses "nightly" as the toolchain key for nightly compilers. Match that
# layout instead of searching .bin, which can select an unrelated cached build.
rust_toolchain_key() {
	local release host

	release="$(rustc -vV | sed -n 's/^release: //p')"
	host="$(rustc -vV | sed -n 's/^host: //p')"

	[[ -n "${release}" ]] || fail "rustc -vV did not report a release"
	[[ -n "${host}" ]] || fail "rustc -vV did not report a host"

	if [[ "${release}" == *-nightly ]]; then
		printf 'nightly\n'
	else
		printf '%s\n' "${host}"
	fi
}

# Trust a cached binary only when cargo's sibling install manifest names the
# pinned package version and full Git revision, and that install entry owns the
# binary we are about to run. A registry install of the same version has a
# different source and is rejected.
manifest_proves_git_revision() {
	local install_dir="$1"

	if [[ -f "${install_dir}/.crates2.json" ]]; then
		jq -e \
			--arg package "${package_name}" \
			--arg version "${package_version}" \
			--arg rev "${git_rev}" \
			--arg bin "${bin_name}" \
			'(.installs // {}) | to_entries | any(
				(.key | startswith($package + " " + $version + " (git+") and endswith("#" + $rev + ")")) and
				((.value.bins // []) | index($bin) != null)
			)' \
			"${install_dir}/.crates2.json" >/dev/null

		return
	fi

	if [[ -f "${install_dir}/.crates.toml" ]]; then
		awk \
			-v prefix="\"${package_name} ${package_version} (git+" \
			-v suffix="#${git_rev})\"" \
			-v bin="\"${bin_name}\"" '
			BEGIN {
				in_value = 0
				found = 0
			}
			{
				if (index($0, prefix) > 0 && index($0, suffix) > 0) {
					in_value = 1
				}

				if (in_value && index($0, bin) > 0) {
					found = 1
				}

				if (in_value && index($0, "]") > 0) {
					in_value = 0
				}
			}
			END {
				exit found ? 0 : 1
			}
		' "${install_dir}/.crates.toml"

		return
	fi

	return 1
}

resolve_leptoskit() {
	local install_dir binary

	read_bin_metadata
	verify_lockfile_sources

	install_dir="${root}/.bin/rust-$(rust_toolchain_key)/${package_name}/${package_version}"
	binary="${install_dir}/bin/${bin_name}"

	if [[ -x "${binary}" ]] && manifest_proves_git_revision "${install_dir}"; then
		printf '%s\n' "${binary}"

		return 0
	fi

	# --force replaces a same-version registry binary that cargo-run-bin would
	# otherwise keep because its install path does not include the Git source.
	printf 'Installing %s %s from %s at %s\n' \
		"${package_name}" "${package_version}" "${git_url}" "${git_rev}" >&2

	local install_args=(
		--force
		--root "${install_dir}"
		--git "${git_url}"
		--rev "${git_rev}"
		--version "${package_version}"
		--bin "${bin_name}"
	)

	if [[ "${metadata_locked}" == "true" ]]; then
		install_args=(--locked "${install_args[@]}")
	fi

	(
		cd "${root}"
		cargo install "${install_args[@]}" "${package_name}" >&2
	) || fail "cargo install failed for ${package_name} ${package_version} at ${git_rev}; refusing to run a stale or unverified CLI"

	if ! manifest_proves_git_revision "${install_dir}"; then
		fail "installed ${package_name} does not prove git ${git_url} at ${git_rev}; refusing to run it"
	fi

	[[ -x "${binary}" ]] || fail "cargo install did not produce ${binary}"

	printf '%s\n' "${binary}"
}

leptoskit_bin="$(resolve_leptoskit)"

if [[ $# -eq 0 ]]; then
	printf '%s\n' "${leptoskit_bin}"
else
	exec "${leptoskit_bin}" "$@"
fi
