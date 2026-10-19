---
monochange_app: fix
---

# Accept hosted release commits larger than 2 MiB

`POST /api/release-commits` now accepts request bodies up to 32 MiB (`monochange_app_api::RELEASE_COMMIT_BODY_LIMIT`). Before this change, the route used axum's 2 MiB default, so a grouped release that rewrote many changelogs was rejected with `400 Bad Request` (`length limit exceeded`) before it was authenticated. monochange's own release commits already send about 3 MiB of changelog content.

Other API routes keep the 2 MiB default. No configuration change is needed; deploy the new app version before switching a large repository's release workflow to `commit_backend = "hosted"`.
