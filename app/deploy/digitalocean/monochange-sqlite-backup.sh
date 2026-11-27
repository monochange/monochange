#!/usr/bin/env bash
# Release snapshots never expire. Only owned daily snapshots have retention.
set -euo pipefail
umask 077
mode=release
if [[ $# == 1 && "$1" == --daily ]]; then
	mode=daily
elif [[ $# != 0 ]]; then
	echo 'usage: monochange-sqlite-backup [--daily]' >&2
	exit 2
fi
DEPLOY_DIR="${MONOCHANGE_DEPLOY_DIR:-/opt/monochange}"
DB="$DEPLOY_DIR/data/monochange_app.sqlite3"
[[ -f "$DB" ]] || {
	echo 'database missing; no snapshot created' >&2
	exit 1
}
command -v sqlite3 >/dev/null
BACKUP_DIR="$DEPLOY_DIR/backups"
[[ "$mode" == daily ]] && BACKUP_DIR="$BACKUP_DIR/daily"
mkdir -p "$BACKUP_DIR"
chmod 0750 "$BACKUP_DIR"
# Reserve an owned directory atomically; SQLite shell quoting preserves spaces
# and quotes without treating a path as another dot-command argument.
owned="$(mktemp -d "$BACKUP_DIR/snapshot-XXXXXXXXXXXX")"
dest="$owned/monochange_app.sqlite3"
validated=0
cleanup() {
	local code=$?
	if [[ "$code" != 0 ]]; then
		rm -f "$owned/prune-list"
		if [[ "$validated" == 0 ]]; then
			rm -f "$dest"
			rmdir "$owned"
		else
			echo "retention failed; validated snapshot retained at $dest" >&2
		fi
	fi
}
trap cleanup EXIT
escaped="${dest//\'/\'\'}"
sqlite3 "$DB" ".backup '$escaped'" >/dev/null 2>&1 || {
	echo 'SQLite online backup failed' >&2
	exit 1
}
chmod 0640 "$dest"
quick="$(sqlite3 "$dest" 'PRAGMA quick_check;' 2>/dev/null)" || {
	echo 'snapshot quick_check command failed' >&2
	exit 1
}
integrity="$(sqlite3 "$dest" 'PRAGMA integrity_check;' 2>/dev/null)" || {
	echo 'snapshot integrity_check command failed' >&2
	exit 1
}
fk="$(sqlite3 "$dest" 'PRAGMA foreign_key_check;' 2>/dev/null)" || {
	echo 'snapshot foreign_key_check command failed' >&2
	exit 1
}
[[ "$quick" == ok && "$integrity" == ok && -z "$fk" ]] || {
	echo 'SQLite snapshot validation failed' >&2
	exit 1
}
sync -f "$dest"
validated=1
if [[ "$mode" == daily ]]; then
	# Never traverse or delete an unrelated file, legacy snapshot or release backup.
	find "$BACKUP_DIR" -mindepth 1 -maxdepth 1 -type d -name 'snapshot-????????????' -mtime +14 -print0 >"$owned/prune-list"
	while IFS= read -r -d '' old; do
		rm -- "$old/monochange_app.sqlite3"
		rmdir -- "$old"
	done <"$owned/prune-list"
	rm "$owned/prune-list"
fi
printf '%s\n' "$dest"
