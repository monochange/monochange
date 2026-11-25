---
monochange_app: fix
---

# Stop the release image check failing on a large changelog

`app/deploy/verify-image.sh` matched the served changelog by piping `printf` into `grep -q`. Once that page grew past the 64 KB Linux pipe buffer, grep could exit on its first match while the writer still had bytes to send; the resulting `EPIPE` failed the script under `pipefail` even though the version was present. The match now reads a here-string, so no pipe can break, and the comment states the rule instead of the previous partial fix.

`app-ci` now passes the workspace version from `app/Cargo.toml` to the script. Pull requests and merge-queue runs therefore exercise the release identity and changelog checks that previously ran only during a real release deploy, so a regression here fails before the release instead of during it.
