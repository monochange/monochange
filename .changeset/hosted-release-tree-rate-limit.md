---
monochange: fix
monochange_github: fix
---

# Build hosted release trees without tripping GitHub's secondary rate limit

Large hosted releases no longer fail. `monochange run release --commit --create-pr` with `MONOCHANGE_RELEASE_BACKEND=hosted` used to upload one `POST /git/blobs` request per release file. A release touching a hundred crate changelogs plus manifests and lockfiles exceeded GitHub's secondary rate limit for content creation, which is about 80 content-creating requests per minute. The resulting `403` was then reported as a missing `contents: write` permission. The fallback that sends file contents to the monochange app then ran past the 30 second provider timeout with `hosted CommitRelease request failed: error sending request for url (…/api/release-commits)`.

`monochange_github::create_release_tree` now sends UTF-8 files inline in the `POST /git/trees` request, so GitHub creates their blobs server-side and the whole release usually needs one tree request. Only non-UTF-8 files are uploaded as separate base64 blobs. Deletions (`"sha": null`) and file modes (`100644`, `100755`, `120000`) are unchanged. A release with more than 4 MiB of inline text is split into tree requests chained through `base_tree`. The GitHub Actions verified-commit path for release pull requests now also inlines its files instead of creating one blob each.

A `429`, or a `403` whose message names a rate limit, is now retried after 20, 40, and 60 seconds. If GitHub still rate limits the write, the warning names the rate limit before the release falls back to sending contents to the app:

```text
sending release file contents to the monochange app because the release tree could not be built: GitHub kept rate limiting git object writes in owner/repo after 3 retries (status 403: You have exceeded a secondary rate limit …)
```

Only a `403` without a rate-limit message still asks you to grant the job `contents: write`. The hosted `POST /api/release-commits` request now has a 5 minute timeout of its own. Other provider calls keep the 30 second `PROVIDER_HTTP_TIMEOUT`.
