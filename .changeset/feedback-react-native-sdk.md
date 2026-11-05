---
"@monochange/feedback-react-native": major
---

# Add native feedback screens to React Native apps

The first release exports `FeedbackSheet` and the shared `FeedbackClient`. The native sheet supports assistant conversations, similar requests, voting, submission, discussion, and delivery updates, with keyboard handling, accessibility labels, host translations, and light or dark themes.

```tsx
<FeedbackSheet
	client={feedbackClient}
	visible={open}
	identityKey={user.id}
	onClose={() => setOpen(false)}
/>;
```

Use a backend-issued mobile session token. Change `identityKey` on logout or account switches so the previous user's draft and credentials are cleared. No WebView is required.
