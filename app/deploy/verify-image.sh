#!/usr/bin/env bash
# Exercise the built image without accessing any production credentials.
# MONOSECRET_PROVIDER=env routes every production secret, including the
# bootstrap token the entrypoint exports, to test values in the environment.
set -euo pipefail
image="${1:?image required}"
version="${2:-}"
token="$(mktemp)"
name="monochange-release-smoke"
base="http://127.0.0.1:3000"
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
	-e MONOSECRET_PROFILE=production -e MONOSECRET_PROVIDER=env \
	-e OP_SERVICE_ACCOUNT_TOKEN_FILE=/run/secrets/onepassword_service_account_token \
	-e DATABASE_URL=sqlite:///data/monochange_app.sqlite3 \
	-e JWT_SECRET=ci-docker-test-signing-key-not-a-secret \
	-e TOKEN_ENCRYPTION_KEY=ci-docker-test-token-encryption-key \
	-e GITHUB_CLIENT_ID=ci-docker-client -e GITHUB_CLIENT_SECRET=ci-docker-client-secret "$image"
curl --fail --silent --show-error --retry 20 --retry-all-errors --retry-delay 1 \
	--retry-max-time 60 --max-time 5 http://127.0.0.1:3000/health | jq -e '.status == "ok" and .http == "up"'
docker exec "$name" sh -c 'test "$(awk "/^Uid:/{print \$2}" /proc/1/status)" = 1000'
docker exec "$name" sh -c 'test "$(awk "/^NoNewPrivs:/{print \$2}" /proc/1/status)" = 1'

# Both frontend namespaces ship in one deployment tree (pkg/site for the
# website entry, pkg/projects for the product entry), the shell-referenced
# public assets, and the deployed WOFF2 fonts.
for path in / /changelog /book /book/guide/02-setup \
	/book-assets/branding/logo-280.png /icons/github.svg /icons/rust.svg \
	/favicon.ico /favicon.svg /site.webmanifest /branding/mark.svg /branding/social.png \
	/fonts/bricolage-800.woff2 /fonts/hanken-400.woff2 /fonts/hanken-700.woff2 \
	/pkg/site/monochange_app.js /pkg/site/monochange_app_bg.wasm \
	/pkg/site/monochange_app.css /pkg/site/__wasm_split_manifest.json \
	/pkg/projects/monochange_projects_web.js /pkg/projects/monochange_projects_web_bg.wasm \
	/pkg/projects/__wasm_split_manifest.json; do
	curl --fail --silent --show-error "${base}${path}" -o /dev/null
done

# Every split chunk each deployed manifest references must be served: read
# each namespace's manifest out of the running container and request its
# chunks over HTTP, so the check exercises the exact files the image carries.
for namespace in site projects; do
	chunks="$(docker exec "$name" sh -c "cat /app/site/pkg/$namespace/__wasm_split_manifest.json" |
		jq -r '[.[] | .[]] | unique | .[]')"
	test -n "$chunks"
	while IFS= read -r chunk; do
		curl --fail --silent --show-error "${base}/pkg/${namespace}/${chunk}.wasm" -o /dev/null
	done <<<"$chunks"
done

# The publisher precompressed every compressible asset; the file handler must
# offer the twins to compression-capable clients instead of raw bytes, and it
# must serve both deployed encodings.
for encoding in gzip br; do
	curl --fail --silent --show-error -D - -o /dev/null \
		-H "Accept-Encoding: $encoding" "${base}/pkg/site/monochange_app_bg.wasm" |
		grep -iE "^content-encoding: $encoding"
	curl --fail --silent --show-error -D - -o /dev/null \
		-H "Accept-Encoding: $encoding" "${base}/pkg/site/monochange_app.css" |
		grep -iE "^content-encoding: $encoding"
done

if [ -n "$version" ]; then
	curl --fail --silent --show-error "${base}/releases/$version.json" |
		jq -e --arg version "$version" '.title == $version'
	curl --fail --silent --show-error "${base}/changelog" | grep -Fq "v$version"
fi
