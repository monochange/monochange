#!/usr/bin/env bash
# Forced command for the website-only Actions SSH key; install root-owned.
set -euo pipefail

if [[ ! "${SSH_ORIGINAL_COMMAND:-}" =~ ^deploy\ ([0-9a-f]{40})\ ([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
	echo 'expected deploy <40-character commit SHA> <stable version>' >&2
	exit 1
fi
revision="${BASH_REMATCH[1]}"
version="${BASH_REMATCH[2]}"
exec 9>/opt/monochange/.deployment.lock
flock -n 9 || {
	echo 'another website deployment is running' >&2
	exit 1
}

if [ -f /opt/monochange/.website-version ]; then
	current="$(cat /opt/monochange/.website-version)"
	newest="$(printf '%s\n%s\n' "$current" "$version" | sort -V | tail -1)"
	if [ "$newest" != "$version" ]; then
		echo 'refusing to replace a newer website release' >&2
		exit 1
	fi
fi

gunzip | docker load
image="monochange-app:$revision"
docker image inspect "$image" >/dev/null
/usr/local/bin/monochange-sqlite-backup
/usr/local/bin/monochange-deploy "$image"
printf '%s\n' "$version" >/opt/monochange/.website-version
