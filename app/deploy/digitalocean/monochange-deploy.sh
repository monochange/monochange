#!/usr/bin/env bash
# Health-checked rollout of one locally loaded monochange.dev image.
#
# Install root-owned as /usr/local/bin/monochange-deploy with mode 0755.
# harden-droplet.sh installs it on a new Droplet; app/DEPLOY.md shows how to
# replace it on the running server. ci-deploy.sh (the forced command for the
# Actions deployment key) and manual deployments both call it.
set -euo pipefail
cd /opt/monochange

IMAGE="${1:?usage: monochange-deploy monochange-app:<commit-sha>}"
if [[ ! "$IMAGE" =~ ^monochange-app:[0-9a-f]{40}$ ]]; then
	echo 'expected monochange-app:<40-character commit SHA>' >&2
	exit 1
fi
docker image inspect "$IMAGE" >/dev/null

# Compose reads the image tag from .env, which this helper owns. Operator
# runtime settings belong in app.env, which docker-compose.yml loads into the
# app container and which this helper never touches.
printf 'MONOCHANGE_IMAGE=%s\n' "$IMAGE" >.env
docker compose up -d --pull never
docker compose ps

# Startup resolves secrets from 1Password and runs database migrations, which
# can take longer than a few seconds on the 1 vCPU Droplet.
if ! curl --fail --silent --show-error --retry 90 --retry-all-errors \
	--retry-delay 1 --retry-max-time 120 --max-time 5 http://127.0.0.1:3000/health; then
	echo >&2
	echo "monochange_app did not report healthy within 120 seconds; startup logs follow." >&2
	docker compose ps app >&2 || true
	docker compose logs --no-color --tail=60 app >&2 || true
	exit 1
fi
echo
