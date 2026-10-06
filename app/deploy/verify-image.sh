#!/usr/bin/env bash
# Exercise the built image without accessing any production credentials.
set -euo pipefail
image="${1:?image required}"
version="${2:-}"
token="$(mktemp)"
name="monochange-release-smoke"
cleanup() {
	docker logs "$name" 2>/dev/null || true
	docker rm -f "$name" >/dev/null 2>&1 || true
	rm -f "$token"
}
trap cleanup EXIT
printf '%s' ci-service-account-test-token >"$token"
chmod 600 "$token"
docker run -d --name "$name" -p 127.0.0.1:3000:3000 \
	--mount "type=bind,source=$token,target=/run/secrets/onepassword_service_account_token,readonly" \
	-e SECRETSPEC_PROFILE=production -e SECRETSPEC_PROVIDER=env \
	-e OP_SERVICE_ACCOUNT_TOKEN_FILE=/run/secrets/onepassword_service_account_token \
	-e DATABASE_URL=sqlite:///data/monochange_app.sqlite3 \
	-e JWT_SECRET=ci-docker-test-signing-key-not-a-secret \
	-e GITHUB_CLIENT_ID=ci-docker-client -e GITHUB_CLIENT_SECRET=ci-docker-client-secret "$image"
curl --fail --silent --show-error --retry 20 --retry-all-errors --retry-delay 1 \
	--retry-max-time 60 --max-time 5 http://127.0.0.1:3000/health | jq -e '.status == "ok" and .http == "up"'
docker exec "$name" sh -c 'test "$(awk "/^Uid:/{print \$2}" /proc/1/status)" = 1000'
docker exec "$name" sh -c 'test "$(awk "/^NoNewPrivs:/{print \$2}" /proc/1/status)" = 1'
for path in / /changelog /pkg/monochange_app.js /pkg/monochange_app.wasm /pkg/monochange_app.css; do
	curl --fail --silent --show-error "http://127.0.0.1:3000$path" -o /dev/null
done
if [ -n "$version" ]; then
	curl --fail --silent --show-error "http://127.0.0.1:3000/releases/$version.json" |
		jq -e --arg version "$version" '.title == $version'
	curl --fail --silent --show-error http://127.0.0.1:3000/changelog | grep -Fq "v$version"
fi
