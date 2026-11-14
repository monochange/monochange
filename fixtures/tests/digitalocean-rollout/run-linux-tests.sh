#!/usr/bin/env bash
# Fixed-path tests install fixture helpers only inside this throwaway container.
set -euo pipefail
if [[ ! -e /.dockerenv && ! -e /run/.containerenv ]]; then
	echo 'deployment helper tests require an isolated Linux container' >&2
	exit 2
fi
apt-get update
apt-get install -y --no-install-recommends sqlite3 jq curl util-linux
for tool in node which sqlite3 jq curl flock; do
	command -v "$tool" >/dev/null
done
export ROLLOUT_FIXED_PATHS=1
node node_modules/vitest/vitest.mjs run scripts/npm/tests/digitalocean-rollout.test.ts
