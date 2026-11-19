---
monochange_github: fix
---

# Wait as long as GitHub asks before retrying rate-limited release tree writes

`monochange_github::create_release_tree` now reads GitHub's rate-limit headers when a git object write is rate limited, instead of retrying on a fixed schedule. A `retry-after` header sets the wait. Without one, an exhausted limit (`x-ratelimit-remaining: 0`) waits until `x-ratelimit-reset`. When GitHub gives no hint, the retries now wait 60, 120, and 240 seconds, because GitHub asks clients to wait at least a minute after a secondary rate limit and to back off exponentially. The previous 20 and 40 second retries came back while the limit still applied, and GitHub warns that requests sent while rate limited can get an integration banned.

If GitHub asks for a wait longer than 5 minutes, the release stops retrying and falls back to sending the release files to the monochange app, with a warning that names the requested wait:

```text
sending release file contents to the monochange app because the release tree could not be built: GitHub rate limited git object writes in owner/repo and asked for a 600s wait, longer than the 300s monochange allows (status 403: You have exceeded a secondary rate limit …)
```
