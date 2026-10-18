# Website authentication and repository flows

Status: production 0.2.2 published; authenticated session verified live; public-page follow-up and GitHub App configuration pending.

The original failure was a successful GitHub sign-in followed by a signed-out dashboard. An earlier live audit stopped at GitHub's login page and did not establish an authenticated session. The repaired app was released through https://github.com/monochange/monochange/pull/765 after direct human authorization; the previous release at https://github.com/monochange/monochange/pull/760 is closed.

## Acceptance checks

- Complete the real GitHub callback, then navigate and reload the dashboard without losing the signed-in state.
- Show the same session in desktop/mobile navigation and dashboard; sign-out expires it and hides protected data.
- Reject absent, invalid, expired and mismatched OAuth/session credentials without revealing another user's repositories.
- Load connected public/private repositories only for the authenticated user; cover empty, suspended and inaccessible installations.
- Trace repository installation, selection and configuration. The repaired dashboard exposes a verified installation link when configured, with explicit unavailable/error states. Real GitHub App registration and installation must be completed before claiming production repository connection.
- Check public home/install/pricing/book/changelog/navigation/theme/error routes in a real browser; distinguish local verification from live deployment.

## Execution

1. Reproduce callback cookie delivery through the actual SSR HTTP response, with a delayed deterministic GitHub response.
2. Add failing session/repository authorization tests against the real database and server-function/SSR routing.
3. Repair the cookie/session/UI root causes and fill authorized setup gaps using verified GitHub identity and permissions.
4. Run app tests, SSR/WASM builds, UI journeys and executable patch coverage; run required fix/lint/validate/release-preview/affected-path checks.
5. Submit a sole Ifiok-authored, GPG-signed ordinary PR; wait for all checks and normal queue merge.
6. Review the automatically refreshed release and obtain concrete human authorization when required. Observe automatic deployment and verify the exact live version plus authenticated critical flows before declaring completion.

## Boundaries

GitHub is the external boundary replaced in deterministic regression tests; cookies, routing, persistence and authorization remain real. Do not read production secret providers to run local tests. Keep authenticated/private browser evidence local and redact tokens/codes. Preserve existing worktrees, stashes and shared caches. No manual release/publishing workflow retries, tag mutations or registry operations.

## Findings and verification

- A real production sign-in using Ifiok's existing Chrome profile reached "Signed in! Welcome, ifiokjr" and then a signed-out dashboard. The earlier live audit had stopped at GitHub login and did not verify this callback.
- Delayed GitHub callback regressions demonstrated that streamed SSR sent headers before the session cookie was appended. Blocking login/callback resources now deliver cookies before redirecting. Navbar session state, logout, repository cookie naming and repository lookup are repaired.
- Session readers verify both database identity and GitHub identity. Personal repositories are scoped to the installation owner. Organization repositories also require current active organization-owner membership, so an old installation cannot preserve access after an owner leaves.
- Signed installation events now establish a real user before creating installation/repository rows, preserve an existing OAuth identity and plan, and update transactionally. Event names and actions must match; unrelated signed events and cross-installation removals cannot mutate another workspace.
- The dashboard resource is created before Suspense to avoid unfinished SSR responses. Test route discovery is cached once, matching production startup and avoiding the framework's process-wide resource suppression race.
- Native app workspace tests: 162 passed, one manual browser fixture ignored. Executable Rust patch coverage: 293/293 (100%). Root fix/lint, app clippy, SSR/WASM compilation, configuration validation, release preview and explicit affected-path verification passed. Final hooks and CI remain required.
- Chrome with the real local HTTPS app, browser cookies and SQLite verified callback, dashboard reload, logout, protected-data removal, configured installation link, both themes and navigation. Widths 320/375/390/768/900/901/1024/1440 fit after moving account navigation into the mobile menu at tablet widths. The expanded installation commands scroll within their containers. All 55 integrated book destinations returned 200; public home/install/pricing/changelog/book pages, syntax highlighting and the canonical docs redirect worked.
- The local browser fixture replaces only external GitHub responses and uses generated test-only keys. Its frontend uses the documented temporary `disable-erase-components` setting to match the independently compiled native test server; that setting was removed afterward. This is a harness build alignment, not a production code change.
- Read-only GitHub settings show no registered App in the monochange organization and only the unrelated kickjump-bot App in Ifiok's personal account. The secret-free registration checklist is prepared in `app/deploy/github-app-registration.md`. No live GitHub registration, permission grant, credential generation/provider access or repository mutation was performed.

Local proof is retained under `.monochange/local/auth-*` and `repository-connection-*`. Local tests did not establish a production deployment or live repository installation; the completed production checks and remaining limits are recorded below.

## Live release follow-up, 8 October 2026

- Automatic deployment from release merge `62d1504b931a4885dc4248d3ea1ef4a19ef61dac` passed, and public health reports 0.2.2. The public app GitHub release contains the reviewed website-stream notes. No manual app publication was needed.
- Real Chrome GitHub OAuth using the existing grant reached the authenticated dashboard, survived reload and signed out correctly. The production dashboard reports that GitHub App configuration is unavailable; no new credentials, permissions or installations were created during verification.
- All 55 book destinations, the `/docs` redirect and current/historical release JSON passed live checks. Homepage, book and expanded Cargo/Nix instructions fit desktop and phone widths in both themes.
- Live verification exposed two remaining public-page defects: the installation setup-guide link still used a malformed standalone-book URL, and the changelog's mobile grid expanded to its longest command's intrinsic width. Correct the link to the shipped chapter and give the mobile grid a zero minimum track size; preserve scrolling inside command blocks.
- The link regression must fail against the old rendered installation page and pass against the correction. Check the corrected link and changelog at 320/375/390/768/900/901/1024/1440 widths in the existing browser harness. Keep the production defect evidence separate from local correction evidence; these follow-up changes require a subsequent approved app release before they are live.
- Follow-up local verification passed: the rendered-page link test failed before the correction, then all 163 native app workspace tests and app clippy passed. The changed executable line has 1/1 coverage. The corrected guide link reached the real chapter on desktop and phone widths; install and changelog passed 32 width/theme measurements without page overflow, while long commands retained internal scrolling. No application console errors were captured. The owned fixture server was stopped, and temporary frontend metadata and viewport overrides were restored. Proof is retained under `.monochange/local/website-polish-*`.
- Sole-maintainer release PR authorship is repaired by https://github.com/monochange/monochange/pull/766. The release-finalization job reached its ten-minute deadline during cache cleanup after successful release operations; https://github.com/monochange/monochange/pull/767 increases that budget. The app deployment itself succeeded.

## Additional flow regressions, 8 October 2026

The maintainer authorized continuing until the reported issues are fixed and requested extensive tests. The next release must include the public link/mobile corrections from https://github.com/monochange/monochange/pull/769 and the named Changelog navigation and migration-guide link corrections from https://github.com/monochange/monochange/pull/770. Review the actual refreshed release record and every applicable check before normal queue merge; verify the resulting version and corrected pages live.

Add two native journeys across the real SSR and API routers, shared SQLite state and browser session cookie. Replace only external GitHub responses. The personal-account journey begins with an empty workspace, delivers signed installation and repository-selection webhooks, reloads the dashboard after each change, and checks public/private repositories, additions after initial setup, removal, suspension, resumption, uninstall and account isolation. The verified connection link must remain available with both empty and populated lists. The organization journey reloads after ownership is revoked and after OAuth authorization expires; protected repository names must disappear and provider failure must not masquerade as an empty workspace.

The existing callback/reload/logout, invalid session, app metadata failure, webhook signature/event validation and ownership tests remain part of the surrounding suite. These deterministic journeys do not substitute for real production installation. GitHub App registration and new credential entry still require the maintainer's browser handoff; keep live connection verification pending until the real App is configured.

Local verification passed with 165 native app workspace tests, including both new journeys, and one existing manual browser fixture ignored. Root formatting/lint checks, app clippy, configuration validation and the read-only release preview passed. The test note appears only in the developer output. No production executable code changed. The generated lockfile's unrelated path-package version refreshes are preserved locally and excluded from this change.
