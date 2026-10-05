#!/usr/bin/env sh
set -eu

if [ -n "${OP_SERVICE_ACCOUNT_TOKEN_FILE:-}" ]; then
	OP_SERVICE_ACCOUNT_TOKEN="$(cat "${OP_SERVICE_ACCOUNT_TOKEN_FILE}")"
	if [ -z "${OP_SERVICE_ACCOUNT_TOKEN}" ]; then
		echo "1Password service account token file is empty" >&2
		exit 1
	fi
	export OP_SERVICE_ACCOUNT_TOKEN
fi

if [ "$(id -u)" -eq 0 ]; then
	exec setpriv --reuid=app --regid=app --init-groups \
		--bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs -- "$@"
fi

exec "$@"
