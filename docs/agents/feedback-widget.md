# Feedback widget integration

Start with the [installation guide](../src/guide/feedback-widget.md). It is the canonical source for copyable iframe, website, backend token endpoint, React Native, and Flutter examples. Link to those examples rather than duplicating them; use an mdt provider if content must render in more than one document.

## Choose the identity contract first

- Paste-only iframe: visitors sign in through monochange in a first-party popup. The host supplies only the iframe URL and sandbox permissions. It needs no host backend or SDK.
- Website SDK or script: the host backend authenticates its own user and issues `web` sessions. The registered origin must match the host page exactly.
- React Native or Flutter: the host backend issues `mobile` sessions. Keep native controls and clear account state through `identityKey`.

Use organization/project slugs in iframe URLs and a numeric Project ID in SDK options. Backend credentials and model keys stay server-side. Package installation requires published SDKs; the iframe requires the matching deployed assets and API. Neither merge nor documentation updates authorize package publication or deployment.

## Find the implementation

| Concern                                        | Source                                                                   |
| ---------------------------------------------- | ------------------------------------------------------------------------ |
| Web transport, widget, hosted iframe and popup | `packages/monochange__feedback/src/{client,widget,hosted}.ts`            |
| Plain HTML and React integration examples      | `packages/monochange__feedback/examples/`                                |
| Hosted frame markup and stylesheet             | `packages/monochange__feedback/embed/`                                   |
| Native React Native sheet                      | `packages/monochange__feedback-react-native/src/index.tsx`               |
| Flutter client and sheet                       | `packages/monochange__feedback_flutter/lib/`                             |
| Client API, token checks, popup exchange       | `app/crates/monochange_app_api/src/embedded_api/{client,auth,iframe}.rs` |
| Integration registration and plan approval     | `app/crates/monochange_app_api/src/embedded_api/admin.rs`                |
| Credentials, sessions, quotas and draft state  | `app/crates/monochange_app_feedback/src/embedded.rs`                     |
| Bundle generation and size budgets             | `scripts/build-feedback.ts`                                              |

Edit hosted markup under the package directory and rebuild it; `app/public/embed/` contains generated deployment assets. Preserve the popup's exact origin, source-window, and channel checks. Return only feedback tokens to the frame; never send credentials to its embedding parent or place them in a URL.

## Validate the change locally

For documentation-only changes, run formatting, `docs:check`, `build:book`, and workspace/changeset validation. No executable-line coverage is needed when no executable lines change. For widget behavior changes, start with a failing regression test and run these focused checks before the normal repository-wide checks:

```sh
devenv shell -- pnpm check
devenv shell -- pnpm vitest run --exclude '.monochange/**' --exclude 'worktrees/**' scripts/npm/tests/feedback-client.test.ts scripts/npm/tests/feedback-widget.test.ts scripts/npm/tests/feedback-hosted.test.ts
devenv shell -- pnpm build:feedback
devenv shell -- cargo test --manifest-path app/Cargo.toml -p monochange_app_api --lib
devenv shell -- cargo clippy --manifest-path app/Cargo.toml -p monochange_app_api --all-targets -- -D warnings
```

`pnpm build:feedback` regenerates the deployed assets and enforces both browser bundle budgets. Add native SDK checks when touching those packages; see [tooling](tooling.md). Keep patch coverage at 100% for executable changes, and validate changeset streams before updating a PR. Live first-party GitHub sign-in, blocked-popup fallback, and account-switch behavior require a deployed integration smoke check; unit tests do not prove an OAuth deployment is configured correctly.
