---
monochange_app: feat
---

# Commit release trees built by the workflow

`POST /api/release-commits` accepts a `tree` instead of `files`. The server checks that the request names exactly one of them and that `tree` is a git object id, then creates the release commit on that tree without uploading any blobs. A tree whose `.github/workflows` differs from the base commit's is refused with `422 Unprocessable Entity` before anything is written, keeping the rule that the bot never changes GitHub workflows.
