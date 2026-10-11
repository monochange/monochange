---
monochange_app: feat
---

# Advance project feedback from pull request and release webhooks

`POST /api/github/webhooks` now handles `pull_request` and `release` events for every project that includes the event's repository:

| Event                                            | Effect                                                                                                                                                                                                                                          |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pull_request` `opened`, `reopened`, or `edited` | `FeedbackService::observe_pull_request` moves accepted or building items to review when the body closes their issue in the same repository (`Fixes #12`, `Fixes owner/name#12`, or an issue URL) or names them with a `Feedback-Item:` trailer. |
| `pull_request` `closed` with `merged: true`      | Calls `observe_merge`.                                                                                                                                                                                                                          |
| `release` `published` (not prereleases)          | Calls `ship_merged`. The version is the tag without a path prefix or leading `v` (`web/v1.2.0` → `1.2.0`); the notes link is the release page.                                                                                                  |

New public API:

- `monochange_app_feedback::closing_issue_numbers` and `feedback_trailers` parse pull request bodies.
- `IssueRef::is_in` checks an issue's repository.
- `monochange_app_api::feedback::Delivery` and `observe_delivery` apply an event to each affected project.

`monochange_app_api::feedback::update` no longer saves when an operation leaves the state unchanged and produces no notifications, so unrelated webhook traffic doesn't bump document versions.

Existing GitHub App registrations must subscribe to the `release` event. No new permission is required.
