---
monochange_app: fix
---

# Explain how to recover when a hosted release pull request fails verification

Hosted `OpenReleaseRequest` updates (`POST /api/hosted_release_request` and `POST /api/release-requests`) that reject an open pull request with HTTP 422 (`GitHubAppError::UnboundPullRequest`) now explain why verification failed and how to recover: the pull request was not created by this deployment's verified flow (for example, it predates release-PR verification, was opened by a user or another bot, or the deployment's signing secret changed), and operators recover by closing that pull request once and re-running the release workflow so monochange opens a verified one.

The verification checks (`verify_identity` and `verify_binding`), HTTP 422 status code, and `GitHubAppError::UnboundPullRequest` variant are unchanged. `app/DEPLOY.md` now documents the one-time close-and-recreate step for open release pull requests in the `monochange_app` 0.5.0 operator release notes and beside `JWT_SECRET`.
