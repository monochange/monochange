# Website feedback

Install `@monochange/feedback` for an accessible Shadow DOM widget or a framework-independent API client. It supports conversations, similar requests, voting, submission, replies, and delivery status.

Read the [installation guide](https://github.com/monochange/monochange/blob/main/docs/src/guide/feedback-widget.md) for registration, authenticated backend token issuance, custom launchers, themes, translations, and script-tag installation.

For a headless integration:

```ts
import { FeedbackClient } from "@monochange/feedback/client";

const feedback = new FeedbackClient({
	endpoint: "https://monochange.dev",
	project: 123,
	getClientToken: () => authenticatedHostApi.feedbackSession("web"),
});
const matches = await feedback.suggest("Export invoices to CSV");
// Ask the user before voting for a match or submitting a new request.
```

The host backend credential belongs on the backend. Call `reset()` on account changes and `destroy()` when disposing the client. Session errors expose `code`, `status`, and `retryAfter`; interrupted submissions reuse their original idempotency key. No browser storage is used.
