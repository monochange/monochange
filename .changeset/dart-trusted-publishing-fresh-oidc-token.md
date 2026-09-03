---
monochange_publish: minor
---

# mint a fresh pub.dev OIDC token immediately before every dart trusted publish

`monochange step publish-packages` (and every workflow that publishes Dart packages through monochange) now mints a fresh GitHub Actions OIDC token right before each `dart pub publish` invocation when the package is opted into pub.dev trusted publishing. Runtime output for successful publishes is unchanged.

GitHub OIDC tokens expire after five minutes ([actions/toolkit#2048](https://github.com/actions/toolkit/issues/2048)), but `dart-lang/setup-dart` mints the pub.dev token once, during workflow setup. Multi-ecosystem release runs that spend minutes on cargo/npm/deno work before reaching the dart publish step then present a token that pub.dev rejects with `Invalid JWT token: invalid timestamps` — and the pub client deletes the stored credential, so the next run cannot even find it. monochange now performs the same exchange setup-dart performs at setup time, but per publish and seconds before the upload:

1. Detect the runner-provided OIDC endpoint (`ACTIONS_ID_TOKEN_REQUEST_URL` plus `ACTIONS_ID_TOKEN_REQUEST_TOKEN`). When either is missing — local runs, or workflows without `permissions: id-token: write` — behavior is unchanged.
2. Request an audience-`https://pub.dev` JWT from that endpoint and export it as `PUB_TOKEN` in the publish command's environment.
3. Run `dart pub token add https://pub.dev --env-var PUB_TOKEN` so the pub client reads the fresh token from `PUB_TOKEN` at request time (re-registering is idempotent and self-heals credentials that a previous auth failure deleted).

The publish job needs `id-token: write`:

```yaml
# .github/workflows/publish.yml
permissions:
  contents: write
  id-token: write # required for pub.dev trusted publishing
```

If minting fails (for example, because `id-token: write` was not granted), the dart publish fails fast with that recovery hint instead of uploading with a stale credential: `refreshing the pub.dev trusted publishing token for package … failed: … the workflow job must grant permissions: id-token: write`. Packages that rely on stored long-lived credentials or publish outside GitHub Actions with trusted publishing disabled are unaffected.
