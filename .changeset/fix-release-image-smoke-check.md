---
monochange_app: fix
---

# Allow developer-only app releases in image smoke and deployment checks

`app/deploy/verify-image.sh` and `.github/workflows/app.yml` previously assumed every `monochange_app` version writes a website-stream `app/public/releases/<version>.json` artifact. Releases carrying only `default`-stream (`website_developer`) changesets intentionally write `app/developer-changelog.md` without adding a website-stream release file, which caused the `app-ci` `docker` smoke check and production release verification to fail with HTTP 404.

`scripts/resolve-app-release-notes.ts` now validates `app/public/releases/*.json`, `app/changelog.md` parity, and (during deployment) the release record to determine the expected website release-notes version: website-stream releases still require and verify their exact `<version>.json` and `/changelog` entry, while developer-only releases verify `/health` against the new app version and check the latest published website release JSON and `/changelog` card.
