#!/usr/bin/env bash
# Re-freeze the `baseline` skill variant from a git ref.
#
# The baseline is the skill as it existed before the current round of edits, so
# a comparison between `baseline` and `package` shows whether the edits changed
# any outcomes rather than merely changing the text.
#
# Usage:
#   ./freeze-baseline.sh            # freeze from HEAD
#   ./freeze-baseline.sh <ref>      # freeze from an explicit ref
set -euo pipefail

ref="${1:-HEAD}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
source_dir="packages/monochange__skill"
target="${here}/baseline"

if ! git -C "${repo}" cat-file -e "${ref}:${source_dir}/SKILL.md" 2>/dev/null; then
	echo "error: ${ref}:${source_dir}/SKILL.md not found in ${repo}" >&2
	exit 1
fi

rm -rf "${target}"
mkdir -p "${target}/skills" "${target}/examples"

git -C "${repo}" show "${ref}:${source_dir}/SKILL.md" >"${target}/SKILL.md"
for path in $(git -C "${repo}" ls-tree --name-only "${ref}" "${source_dir}/skills/"); do
	git -C "${repo}" show "${ref}:${path}" >"${target}/skills/$(basename "${path}")"
done
for path in $(git -C "${repo}" ls-tree --name-only "${ref}" "${source_dir}/examples/"); do
	git -C "${repo}" show "${ref}:${path}" >"${target}/examples/$(basename "${path}")"
done

echo "Froze baseline from ${ref} ($(find "${target}" -type f | wc -l | tr -d ' ') files)"
