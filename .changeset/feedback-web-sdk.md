---
"@monochange/feedback": major
---

# Install feedback on any website

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
