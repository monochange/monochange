#!/usr/bin/env bash
# Run only in the dedicated engine rehearsal container, with its own /opt.
set -euo pipefail
umask 077
fixtures=/workspace/fixtures/tests/digitalocean-rollout
root=/opt/monochange
[[ ! -e "$root" ]]
mkdir -p "$root"/{data,backups,secrets,caddy}
for name in monochange-deploy monochange-sqlite-backup; do
 install -m 0755 "/workspace/app/deploy/digitalocean/$name.sh" "/usr/local/bin/$name"
done
install -m 0755 /workspace/app/deploy/digitalocean/ci-deploy.sh /usr/local/bin/monochange-ci-deploy
cp "$fixtures/compose.yml" "$root/docker-compose.yml"
cp "$fixtures/app.env" "$root/app.env"
cp "$fixtures/token.txt" "$root/secrets/token"
cp "$fixtures/Caddyfile" "$root/caddy/Caddyfile"
cp "$fixtures/token.txt" "$root/backups/legacy.sqlite3"
sqlite3 "$root/data/monochange_app.sqlite3" < "$fixtures/source.sql"
export COMPOSE_PROJECT_NAME=rollout-engine-sol-5b49
cleanup() { cd "$root"; docker compose down; }
trap cleanup EXIT
export SSH_ORIGINAL_COMMAND='deploy aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 1.2.3'
# CI must ignore all these hostile overrides while retaining Docker's local socket.
MONOCHANGE_DEPLOY_DIR=/untrusted MONOCHANGE_BACKUP_HELPER=/untrusted MONOCHANGE_HEALTH_TIMEOUT=0 /usr/local/bin/monochange-ci-deploy
[[ "$(find "$root/backups" -name monochange_app.sqlite3 | wc -l)" == 1 ]]
[[ "$(sqlite3 "$root/data/monochange_app.sqlite3" 'SELECT x FROM t;')" == 42 ]]
cmp "$fixtures/app.env" "$root/app.env"
cmp "$fixtures/token.txt" "$root/secrets/token"
cmp "$fixtures/Caddyfile" "$root/caddy/Caddyfile"
cmp "$fixtures/token.txt" "$root/backups/legacy.sqlite3"
jq -e '.outcome == "success" and .attemptedVersion == "1.2.3"' "$root"/deployments/attempt-*/receipt.json
mkdir -p /workspace/.monochange/local/rollout-repair/engine
cp -R "$root/deployments" /workspace/.monochange/local/rollout-repair/engine/
