---
monochange_app: fix
---

# Make manual and CI website rollouts one data-preserving serialized transaction

`monochange-deploy` now owns the deployment lock, an integrity-checked SQLite backup, exact version verification, and per-attempt recovery receipts for both manual and CI deployments. `ci-deploy` validates its wire boundary and delegates to the shared helper without nested locks or duplicate backups. Legacy `.env` settings fail closed without revealing input, and daily SQLite snapshots retain the 14-day policy only in a dedicated namespace.

Operators must reinstall all three helpers, install jq/sqlite3/util-linux/curl/gzip/coreutils and change the daily cron to `monochange-sqlite-backup --daily`. Move legacy optional `.env` settings into `app.env`, retaining only `MONOCHANGE_IMAGE` in `.env`. Manual calls require the full image SHA and stable version; use `--rollback` explicitly for older images. Use `--bootstrap` only for an empty new server; routine deployments now refuse a missing database. Recovery receipts distinguish desired/running/last-verified identity and preserve every release snapshot; no database is restored automatically.

The app CI workflow runs the installed-helper regression suite in an isolated Linux container. Fixed-path verification refuses host installation, and test/fixture changes trigger this check directly.

A daily retention failure reports an error while retaining the newly validated snapshot. Failed SQLite verification commands report the failing pragma without exposing database contents.
