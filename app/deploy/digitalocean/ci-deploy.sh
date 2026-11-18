#!/usr/bin/env bash
# Forced command for the website-only Actions SSH key; install root-owned.
#
# The SSH_ORIGINAL_COMMAND boundary stays exactly:
#   deploy <40-character commit SHA> <stable version>
# This script validates that boundary, then hands off to the single transaction
# owner (monochange-deploy.sh) with --load-stdin so image loading, backup,
# rollout, and bookkeeping all happen under one lock with one backup.
set -euo pipefail

if [[ ! "${SSH_ORIGINAL_COMMAND:-}" =~ ^deploy\ ([0-9a-f]{40})\ ([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
	echo 'expected deploy <40-character commit SHA> <stable version>' >&2
	exit 1
fi
revision="${BASH_REMATCH[1]}"
version="${BASH_REMATCH[2]}"

# Fixed production root: never trust incoming overrides on the forced boundary.
unset MONOCHANGE_DEPLOY_DIR MONOCHANGE_BACKUP_HELPER MONOCHANGE_HEALTH_TIMEOUT
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
exec /usr/local/bin/monochange-deploy --load-stdin "monochange-app:$revision" "$version"
