# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## @monochange/feedback-react-native [0.2.0](https://github.com/monochange/monochange/releases/tag/@monochange/feedback-react-native/v0.2.0) (2026-11-06)

### 💥 Breaking Change

#### Add native feedback screens to React Native apps

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #860](https://github.com/monochange/monochange/pull/860)

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
