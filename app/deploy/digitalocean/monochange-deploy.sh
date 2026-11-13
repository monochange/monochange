#!/usr/bin/env bash
# One lock owns load, backup, migration startup and verified identity promotion.
# Manual rehearsal overrides: MONOCHANGE_DEPLOY_DIR, MONOCHANGE_BACKUP_HELPER,
# MONOCHANGE_HEALTH_TIMEOUT (1..120 seconds). CI explicitly clears these.
set -euo pipefail
umask 077
rollback=0
load_stdin=0
bootstrap=0
case "${1:-}" in
--rollback)
	rollback=1
	shift
	;;
--load-stdin)
	load_stdin=1
	shift
	;;
--bootstrap)
	bootstrap=1
	shift
	;;
esac
if [[ $# != 2 || ! "$1" =~ ^monochange-app:[0-9a-f]{40}$ || ! "$2" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
	echo 'usage: monochange-deploy [--rollback|--load-stdin|--bootstrap] monochange-app:<40hex> <stable X.Y.Z>' >&2
	exit 2
fi
IMAGE="$1"
VERSION="$2"
DEPLOY_DIR="${MONOCHANGE_DEPLOY_DIR:-/opt/monochange}"
BACKUP_HELPER="${MONOCHANGE_BACKUP_HELPER:-/usr/local/bin/monochange-sqlite-backup}"
health_timeout="${MONOCHANGE_HEALTH_TIMEOUT:-120}"
[[ "$DEPLOY_DIR" == /* && -d "$DEPLOY_DIR" && "$BACKUP_HELPER" == /* && -x "$BACKUP_HELPER" ]] || {
	echo 'invalid deployment root or backup helper' >&2
	exit 2
}
[[ "$health_timeout" =~ ^[1-9][0-9]*$ && "$health_timeout" -le 120 ]] || {
	echo 'health timeout must be 1..120 seconds' >&2
	exit 2
}
for cmd in docker curl jq flock sqlite3 sha256sum gunzip; do
	command -v "$cmd" >/dev/null || {
		echo "missing command: $cmd" >&2
		exit 1
	}
done
export MONOCHANGE_DEPLOY_DIR="$DEPLOY_DIR"
cd "$DEPLOY_DIR"
exec 9>.deployment.lock
flock -n 9 || {
	echo 'another website deployment is running' >&2
	exit 1
}

# Never evaluate dotenv as shell or print malformed input (it can be a secret).
previous_desired=""
if [[ -f .env ]]; then
	while IFS= read -r line || [[ -n "$line" ]]; do
		[[ "$line" =~ ^[[:space:]]*(#.*)?$ ]] && continue
		if [[ "$line" =~ ^MONOCHANGE_IMAGE=(monochange-app:[0-9a-f]{40})$ && -z "$previous_desired" ]]; then
			previous_desired="${BASH_REMATCH[1]}"
		else
			echo 'refusing legacy or malformed .env; migrate settings to app.env without evaluating them' >&2
			exit 1
		fi
	done <.env
fi
prior_version=""
if [[ -f .website-version ]]; then
	prior_version="$(cat .website-version)"
	[[ "$prior_version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || {
		echo 'invalid previous version metadata' >&2
		exit 1
	}
fi
last_verified='null'
if [[ -f .website-verified.json ]]; then
	jq -e 'type == "object" and (.image | type == "string" and test("^monochange-app:[0-9a-f]{40}$")) and (.version | type == "string" and test("^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)$"))' .website-verified.json >/dev/null || {
		echo 'invalid verified identity metadata' >&2
		exit 1
	}
	last_verified="$(cat .website-verified.json)"
	[[ "$(jq -r .version <<<"$last_verified")" == "$prior_version" ]] || {
		echo 'version markers disagree; inspect recovery records' >&2
		exit 1
	}
fi
if [[ "$rollback" == 0 && -n "$prior_version" && "$(printf '%s\n%s\n' "$prior_version" "$VERSION" | sort -V | tail -1)" != "$VERSION" ]]; then
	echo 'refusing newer website release replacement; use --rollback explicitly' >&2
	exit 1
fi
DB="$DEPLOY_DIR/data/monochange_app.sqlite3"
if [[ "$bootstrap" == 1 ]]; then
	if [[ -e .env || -e .website-version || -e .website-verified.json || -e deployments || -e "$DB" ]]; then
		echo 'bootstrap refused: prior deployment history or data exists' >&2
		exit 1
	fi
	for directory in data backups; do
		if [[ -e "$directory" ]]; then
			[[ -d "$directory" ]] || {
				echo 'bootstrap history path is not a directory' >&2
				exit 1
			}
			contents="$(ls -A "$directory")" || {
				echo 'cannot inspect bootstrap history' >&2
				exit 1
			}
			[[ -z "$contents" ]] || {
				echo 'bootstrap refused: prior data or backups exist' >&2
				exit 1
			}
		fi
	done
elif [[ ! -f "$DB" ]]; then
	echo 'database missing; routine rollout cannot initialize production data' >&2
	exit 1
fi
mkdir -p deployments
attempt="$(mktemp -d "$DEPLOY_DIR/deployments/attempt-XXXXXXXXXXXX")"
started="$(date -u +%FT%TZ)"
backup_path=""
backup_digest=""
stage=preflight
outcome=running
running_before=""
running_after=""
# Unknown running identity stays unknown; desired .env is never called healthy.
running_identity() {
	local container
	container="$(docker compose ps -q app 2>/dev/null)" || return 0
	[[ -n "$container" ]] || return 0
	docker inspect --format '{{.Config.Image}}' "$container" 2>/dev/null || return 0
}
write_receipt() {
	local finished=""
	[[ "$outcome" == running ]] || finished="$(date -u +%FT%TZ)"
	jq -n --arg started "$started" --arg finished "$finished" \
		--arg outcome "$outcome" --arg stage "$stage" --arg desired "$previous_desired" \
		--arg before "$running_before" --arg after "$running_after" --arg image "$IMAGE" --arg version "$VERSION" \
		--arg backup "$backup_path" --arg digest "$backup_digest" --argjson verified "$last_verified" \
		--argjson rollback "$rollback" \
		'{startedAt:$started,finishedAt:(if $finished == "" then null else $finished end),outcome:$outcome,stage:$stage,rollbackMode:($rollback==1),previousDesiredImage:$desired,runningBefore:$before,runningAfter:$after,lastVerifiedBefore:$verified,attemptedImage:$image,attemptedVersion:$version,backupPath:$backup,backupDigest:$digest}' >"$attempt/receipt.tmp" || return
	mv "$attempt/receipt.tmp" "$attempt/receipt.json" || return
	sync -f "$attempt/receipt.json" || return
	sync -f "$attempt"
}
finish() {
	local code=$?
	trap - EXIT
	[[ "$outcome" != running ]] || outcome=failed
	running_after="$(running_identity)"
	write_receipt || {
		echo 'failed to persist receipt' >&2
		exit 1
	}
	exit "$code"
}
trap finish EXIT
trap 'outcome=interrupted; exit 130' INT
trap 'outcome=interrupted; exit 143' TERM
running_before="$(running_identity)"
write_receipt
if [[ "$load_stdin" == 1 ]]; then
	stage=load
	gunzip | docker load
fi
docker image inspect "$IMAGE" >/dev/null
stage=backup
if [[ "$bootstrap" == 0 ]]; then
	backup_path="$("$BACKUP_HELPER")" || {
		echo 'backup failed before desired image or migrations changed' >&2
		exit 1
	}
	[[ -f "$backup_path" ]] || {
		echo 'snapshot missing' >&2
		exit 1
	}
	# Independent verification also checks command exit status, never just stdout.
	quick="$(sqlite3 "$backup_path" 'PRAGMA quick_check;' 2>/dev/null)"
	integrity="$(sqlite3 "$backup_path" 'PRAGMA integrity_check;' 2>/dev/null)"
	fk="$(sqlite3 "$backup_path" 'PRAGMA foreign_key_check;' 2>/dev/null)"
	[[ "$quick" == ok && "$integrity" == ok && -z "$fk" ]] || {
		echo 'snapshot validation failed' >&2
		exit 1
	}
	backup_digest="sha256:$(sha256sum "$backup_path" | cut -d ' ' -f 1)"
fi
write_receipt
stage=compose
printf 'MONOCHANGE_IMAGE=%s\n' "$IMAGE" >"$attempt/desired.env"
mv "$attempt/desired.env" .env
docker compose up -d --pull never
running_after="$(running_identity)"
stage=health
# SECONDS is Bash's elapsed clock; every request and sleep fits the deadline.
deadline=$((SECONDS + health_timeout))
healthy=0
while ((SECONDS < deadline)); do
	remaining=$((deadline - SECONDS))
	if body="$(curl --fail --silent --show-error --max-time "$remaining" http://127.0.0.1:3000/health 2>/dev/null)" &&
		jq -e --arg version "$VERSION" '.status == "ok" and .http == "up" and .version == $version' <<<"$body" >/dev/null 2>&1; then
		healthy=1
		break
	fi
	((SECONDS < deadline)) && sleep 1
done
[[ "$healthy" == 1 ]] || {
	echo "exact healthy version unavailable within ${health_timeout} seconds; inspect server logs" >&2
	exit 1
}
# Commit point: durable verified identity, then the compatibility version marker.
# A crash between these writes fails closed on the next attempt for operator repair.
stage=commit
jq -n --arg image "$IMAGE" --arg version "$VERSION" --arg receipt "$attempt/receipt.json" '{image:$image,version:$version,receipt:$receipt}' >"$attempt/verified.json"
sync -f "$attempt/verified.json"
mv "$attempt/verified.json" .website-verified.json
sync -f "$DEPLOY_DIR"
outcome=verified
printf '%s\n' "$VERSION" >"$attempt/version"
mv "$attempt/version" .website-version
sync -f .website-version
outcome=success
stage=complete
echo "deployed $IMAGE as version $VERSION"
