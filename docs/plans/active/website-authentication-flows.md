# Website authentication and repository flows

Status: local repairs verified; ordinary PR and production rollout pending.

The user reports a successful GitHub sign-in followed by a signed-out dashboard. The previous live audit stopped at GitHub's login page and did not establish an authenticated session. Public production still uses an older image; the reviewed website release at https://github.com/monochange/monochange/pull/760 awaits human authorization.

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

Local proof is retained under `.monochange/local/auth-*` and `repository-connection-*`. Local tests do not establish a completed production deployment or live repository installation. The generated release must refresh after this ordinary repair merges, be reviewed at its actual head, and receive the required concrete human authorization before rollout.
