# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## @monochange/feedback [0.2.0](https://github.com/monochange/monochange/releases/tag/@monochange/feedback/v0.2.0) (2026-11-08)

### 💥 Breaking Change

#### Install feedback on any website

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #860](https://github.com/monochange/monochange/pull/860)

The first release provides `FeedbackClient` for headless integrations and `mountFeedback` for an accessible Shadow DOM widget. Users can discuss a request, find similar requests, upvote, submit, and follow delivery updates without a GitHub account.

```ts
import { mountFeedback } from "@monochange/feedback";

const feedback = mountFeedback({
	endpoint: "https://monochange.dev",
	project: 123,
	getClientToken: async () => (await fetch("/api/feedback-session")).json(),
	getContext: () => ({ route: location.pathname }),
});
```

Issue short-lived feedback sessions through your authenticated backend. Call `feedback.reset()` on logout or account changes. Backend credentials must never be embedded in client code. Submission retries keep the same idempotency key.

For a feedback page embedded inside an iframe, use `presentation: "inline"`. The form fills its container, omits the floating launcher and close button, and appears without a modal backdrop or automatic focus. Explicit `open()`, `close()`, and account-reset behavior remain available.
