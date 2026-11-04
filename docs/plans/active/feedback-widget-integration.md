# Feedback widget integration and agent plan approval

**Status**: all feature stages implemented. Feature validation, packaging, CLI contracts, and the full website release build/bundle checks pass. Native builds and device verification require a working platform toolchain. **Related**: [Project feedback](feedback-loop-poc.md), [End-to-end feedback flow](end-to-end-feedback-flow.md).

## Goal and flow

Let a website or app install monochange feedback on any screen, supply its own user identity, and collect bug reports, change requests, and feature requests into a connected project. A maintainer reviews an agent's implementation plan before work starts.

1. The host app authenticates its user and obtains a short-lived feedback session through its backend. The integration identifies the monochange project and the source surface, such as web or mobile.
2. The user opens the widget from a launcher or an existing app control and types a message. Before creating an item, the assistant suggests similar requests the user may read and upvote.
3. The user chooses an existing request or submits a new one. Voting is explicit; finding a similar request never silently casts a vote, merges reports, or discards the draft.
4. A new submission enters the existing feedback pipeline. The assistant classifies it, asks for missing details, and records the source app, surface, route or screen, and app version.
5. By default, a maintainer selects a request for repository planning. A maintainer may instead enable automatic planning of eligible feature requests for the project. A connected local or self-hosted agent returns a versioned implementation plan to the monochange console.
6. The maintainer chooses **Approve**, **Request changes**, or **Reject**. Approve authorizes implementation of that exact plan; request changes creates a new revision; reject records a reason without dispatching implementation. Rejecting a plan does not automatically decline the underlying request.
7. The connected local or self-hosted implementation agent receives the approved plan and returns changes for a draft pull request. Existing pull request and release observation updates the request's progress for its submitter and voters.

## Existing implementation to reuse

- `app/crates/monochange_app_feedback/`: registered apps, page context, intake, similarity, triage, discussion, disclosure rules, voting, decisions, and delivery state. There are currently two feedback kinds: bug and feature. Treat a general change request as a feature request in the first version, while allowing triage to classify a reported fault as a bug.
- `app/crates/monochange_app_api/src/feedback.rs`: project-scoped persistence, optimistic concurrency retries, issue creation, and GitHub agent dispatch. It currently loads `RuleBasedTriage`; similarity uses keyword overlap, and neither is an LLM conversation.
- `app/crates/monochange_app/src/server_fns/portal.rs`: public suggestions and GitHub-authenticated submission, replies, and votes. Host-app identity needs a separate entrypoint so an embedded user does not need a monochange or GitHub account.
- `app/crates/monochange_app/src/pages/feedback_console.rs`: maintainer decisions and **Build with an agent**. Today that action dispatches implementation directly, without a versioned plan approval step.
- `app/crates/monochange_app_db/`: SQLite storage, feedback notifications, and programmatic Welds migrations in `src/lib.rs`.

## Confirmed product decisions

- The first release includes the website widget, a React Native SDK, and a Flutter SDK. All three use the same feedback API and provide submission, similar requests, explicit voting, assistant discussion, and status updates.
- Planning and approved implementation run on a connected local or self-hosted agent instance. The initial integration must support pairing and authenticated jobs for that agent; GitHub Actions is an existing execution path, not the default for this feature.
- Maintainers select requests for planning by default. Each project also offers an opt-in **Automatically plan feature requests** setting. Both modes require a maintainer to approve the resulting plan before implementation.

## Integration choices used for implementation

The implementation uses the defaults below following authorization to implement all stages.

| Decision                          | Proposed default                                                                                                                                        | Effect of another choice                                                                                                                                                     |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Submitter identity                | Require an identity attested by the host backend; use its existing signed-in users                                                                      | Guest feedback needs backend-issued guest sessions and stricter quotas; a static site without a backend needs a separate authentication or challenge flow                    |
| Feedback visibility               | Suggest and show only items already allowed on the project's user-facing roadmap; keep drafts, plans, and private discussion in the console             | Customer-specific or private suggestion pools require audience permissions beyond the current public portal model                                                            |
| Model provider and cost ownership | Deployment-configured provider for widget conversations; connected-agent operator supplies the provider and credentials for planning and implementation | Per-project model keys and billing need additional encrypted credential management and quotas; the initial adapter accepts OpenRouter-compatible structured chat completions |
| First connected-agent adapter     | A worker with a provider-neutral job protocol and an operator-configured agent adapter                                                                  | The first worker is an installable Node CLI with its own configurable model/tool loop; attaching an existing third-party agent app requires a separate supported adapter     |

The connected agent is scoped to authorized projects and repositories. Forwarding feedback between separate projects or monochange servers is outside the initial scope.

## Host identity and client token contract

Register each integration under a project with a stable id, display name, allowed web origins, enabled surfaces, identity namespace, and a revocable backend credential. An identity namespace lets a project's web and mobile integrations share the same user and vote when the host confirms that they share an account system. Equal user-id strings from unrelated account systems must remain distinct. Existing GitHub portal identities stay separate unless a future account-linking flow proves ownership of both.

The host backend derives the user id from its authenticated session and exchanges it with monochange for a feedback token. It must not accept an arbitrary user id supplied by the browser as proof of identity. Store backend credentials as hashes and support rotation and immediate revocation. Keep backend credentials off websites and out of app binaries; distributed apps cannot keep a bundled shared secret confidential ([RFC 8252, section 8.5](https://www.rfc-editor.org/rfc/rfc8252.html#section-8.5)).

Proposed feedback tokens expire after ten minutes and bind an issuer, feedback-specific audience and token type, project, integration, pseudonymous subject, surface, allowed operations, and session id. Use a dedicated signing configuration and explicit verification rules so a feedback token can never act as a maintainer session or release token ([RFC 8725](https://www.rfc-editor.org/rfc/rfc8725.html)). Derive the stored voter id from the project's identity namespace and external user id; do not expose raw host ids to other users. The server resolves the project, actor, and app from verified claims rather than trusting these values in submission bodies.

Check integration and session revocation on every authenticated request so outstanding tokens cannot outlive a revoked integration. Keep subject identities stable through credential and signing-key rotation, using a persisted mapping or a separately versioned identity derivation, so rotation does not reset votes.

Use bearer authorization without third-party cookies. Keep the token in memory and refresh through the host backend; clear identity, draft state, and tokens on logout or account changes. Match web origins against the registered allowlist on preflight and actual requests. React Native and Flutter requests may lack an `Origin`, so permit that only for integrations and token surfaces explicitly configured for native access. A missing or `null` origin must never become a blanket browser bypass.

A token identifies and limits a caller; it cannot eliminate spam on its own. Apply quotas by project, integration, verified subject, and trusted client address, with separate costs for search, messages, submissions, and votes. Rate-limit session issuance as well, return `429` with `Retry-After`, and keep user quotas stable across token refresh. Use durable counters for expensive work so restarts and multiple workers cannot reset those budgets. Record request ids and outcomes without logging tokens or raw message text by default.

## Widget and feedback API

Create a proposed `@monochange/feedback` package in `packages/monochange__feedback/`, exporting both a small API client and an optional Shadow DOM widget. Build an installable package artifact and a versioned script bundle, preserving `/embed/feedback.js` as the discovery entrypoint described in existing plans. Package publication and production hosting remain maintainer-controlled release work.

The public SDK contract should support a mount target, custom launcher, theme and locale, an asynchronous `getClientToken` callback, a `getContext` callback, `open`, `close`, `reset`, and `destroy`. Mount in an existing element or use a floating launcher. Provide an example for plain JavaScript and a framework wrapper example; the core package must not depend on React or the host app loading the monochange site's WASM bundle.

Include `@monochange/feedback-react-native` in `packages/monochange__feedback-react-native/` and the Dart package `monochange_feedback` in `packages/monochange__feedback_flutter/` in the first release. Provide native React Native components and Flutter widgets with host-triggered presentation, themed feedback screens, assistant messages, suggestions, vote controls, and status views. Mobile SDKs use native UI rather than requiring a WebView. Share transport and schema types with the TypeScript client where compatible; the Dart client follows the same versioned wire contract. Keep React Native dependencies out of the web package.

Each SDK accepts asynchronous token refresh and host-provided context, clears sessions on account changes, and handles background/resume, expired credentials, interrupted requests, and safe retry. Mobile context uses stable screen and element identifiers rather than requiring DOM selectors. Include native accessibility, keyboard and safe-area behavior, Android and iOS example hosts, and equivalent API contract fixtures for all three SDKs. Platform-specific end-to-end verification uses available device tooling; all three SDKs remain initial release requirements.

REST routes live under `/api/v1/feedback/{project}`:

- `POST /sessions`: backend-authenticated exchange for an integration's user and surface.
- `POST /suggestions`: authenticated, bounded search over visible requests; text stays out of query strings.
- `POST /conversations` and `POST /conversations/{id}/messages`: private draft conversation and follow-up questions, tied to the verified subject and project.
- `POST /items`: explicitly submit a bug or feature request from a reviewed draft, with an idempotency key.
- `GET /items/{id}` and `POST /items/{id}/messages`: authorized status and discussion, with disclosure rules applied.
- `PUT /items/{id}/vote` and `DELETE /items/{id}/vote`: idempotent vote changes for the verified subject.

Draft conversations must not become public roadmap entries automatically. The same user may access their own drafts and held submissions; another subject cannot retrieve them by guessing an id. Use bounded pagination, request sizes, response schemas, and stable error codes. Client retries preserve the same submission idempotency key. Suggestion replies include visible titles, statuses, vote counts, and whether the current user already voted, without leaking private reports or implementation plans.

The first widget supports text and explicitly supplied context: screen or route, app version, locale, and an optional stable element identifier. Strip query strings and fragments from automatically captured URLs, bound all metadata, and allow the host to supply a safe route instead. Screenshots, recordings, DOM scraping, and automatic logs are later opt-ins requiring storage and consent design; object storage must not block the text-only widget.

## Assistant conversation and similarity

Retrieve candidates from the authorized project's visible requests before the model generates a reply. Start with the existing keyword scorer as a fallback, and add a provider adapter for semantic matching and structured conversational replies. Validate returned item ids against the retrieved candidates; the model cannot invent a votable request or broaden the search audience. Semantic matching needs paraphrase examples in its evaluation fixtures.

The assistant explains existing matches, asks concise clarification questions, and offers **Upvote** or **Submit a new request**. Voting and submission remain explicit user actions. A provider timeout leaves the message draft intact and returns available deterministic suggestions with an honest fallback message. The widget must never claim a bug was reproduced unless a real verification run supplies evidence.

Perform external model calls in an asynchronous API or job layer, then persist validated results with a version guard. The current `feedback::update` operation can rerun on a concurrency conflict, so network requests, billing, and agent dispatch must remain outside that retry closure. Reuse disclosure and quarantine checks for user text and generated replies; enforce audience rules on retrieval and rendering as well.

## Plan approval and implementation handoff

Keep implementation plans and agent jobs separate from existing delivery stages so older feedback documents continue to load. A plan has an id and revision, source feedback version, selected repositories and base commits, summary, acceptance criteria, proposed changes, dependencies or migrations, checks, risks, and estimated effort or budget where supported. Add an explicit planning status, including queued, running, awaiting approval, changes requested, rejected, failed, cancelled, and superseded.

### Planning policy

Persist a project planning policy with **Manual selection** as the default and a maintainer-controlled **Automatically plan feature requests** option. Record policy changes in the audit history. The option authorizes planning only; it neither accepts requests on the maintainer's behalf nor grants implementation approval. Bug reports remain manually selected in the first version.

Automatic planning queues a new feature request only after validation and triage are complete, required clarification is answered, screening permits automated processing, and project repository routing identifies an authorized target. Quarantined, declined, closed, and duplicate requests do not queue automatically. Upvotes alone do not create another planning job. The console explains when a request is waiting for clarification, repository selection, worker availability, or budget.

Use a project planning budget, queue and concurrency limits, and one active automatic job per request and material brief revision. Deduplicate repeated intake events and budget reservations; account for each execution attempt, including retries, against the cap. Automatic retries are bounded; a changed brief marks an old plan stale, and automatic replanning consumes a new budget reservation. The worker enforces its own time or model-cost cap as applicable. No approval or build may be inferred from an automatic planning result.

Enabling the setting applies to subsequently eligible feature requests; planning an existing backlog requires an explicit selection. Disabling it prevents new automatic jobs and cancels unclaimed automatic jobs. Running jobs remain visible and may be cancelled explicitly; manual jobs remain available. Jobs queue durably while an authorized agent is offline and are marked running only after a worker claims them.

### Approval and connected execution

The console shows the complete plan and its revision history. Record the approving maintainer, timestamp, plan digest, repository commits, and allowed implementation scope. Approving an item and approving its implementation plan are distinct decisions. An end-user client token cannot perform either action. Changing the plan, its scope, or its material source brief invalidates approval; if a pinned repository base is no longer suitable, regenerate and request approval before implementation.

Both the console and dispatch backend must enforce the approval requirement. For integrations using this flow, replace the direct build action with **Generate plan**, then **Approve and build** once the plan is ready. Include the existing build entrypoint in the migration and tests so it cannot bypass the new gate.

Define a connected-agent handoff carrying job id, job kind, feedback reference and brief revision, repository and pinned commit, plus plan id, revision and digest for implementation jobs. Include an issue reference when an accepted item already has an issue; automatic planning must not require prior acceptance or issue creation. The authenticated worker fetches the assigned planning brief or immutable approved implementation brief. A planning worker may read the selected repository and return a plan, while an implementation worker may return changes for a draft pull request within the approved scope. Both treat feedback as data. Neither worker receives merge or publication authority.

Make local and self-hosted workers the initial execution adapter. A maintainer pairs a worker using an expiring, single-use authorization flow and grants explicit project and repository access. Store only a hash of its revocable worker credential, keep model credentials on the worker, and expose status, capabilities, last contact, and revocation in the console. An ordinary feedback token cannot pair an agent, claim work, or report results. The worker connects outward to monochange over authenticated HTTPS; a developer machine does not need an inbound port.

Persist queued jobs transactionally, claim them with leases, and authenticate progress/results against the worker's assigned job, repository, job kind, and brief or plan revision. Support heartbeats, timeouts, cancellation, retryable failures, and idempotent result handling. Use attempt identifiers and fencing to accept results and pull-request submission only from the current lease, and require workers to stop when their lease is lost. Do not automatically reassign an implementation attempt while its previous worker may still be running; reconcile a confirmed stop or require an explicit maintainer retry. Cancellation is cooperative for local execution; the console distinguishes cancellation requested from a worker-confirmed stop. Reconcile an ambiguous result rather than blindly starting a second run.

The worker uses an isolated repository checkout with explicit resource and tool permissions. Its operator configures the supported agent adapter locally; user feedback cannot choose executables, shell commands, or arbitrary filesystem paths. Planning is limited to producing a plan. After approval, implementation returns a bounded patch and results for validation before a draft PR is opened through the project's repository integration. Repository access is scoped to the assigned work, and local machine secrets are excluded from the agent's execution context. Preserve macOS, Linux, and Windows worker support. The existing GitHub Actions path remains separate; this feature does not require adopting it.

Use the existing pull request and release observation for progress. Mark **Shipped** only when the observed release includes the change; preserve the existing release-association limitation described in [Project feedback](feedback-loop-poc.md#known-limits) and test the supported release path. In-widget polling can display stored updates in the first version; email, push delivery, and live streaming can follow independently.

## Implementation order and affected files

1. **Finalize integration choices.** Use host-attested identities, existing roadmap visibility, deployment-configured OpenRouter-compatible conversations, and the provider-neutral connected worker. Keep web, React Native, Flutter, connected execution, and optional automatic feature planning in initial scope.
2. **Define contracts with failing tests.** Extend `monochange_app_feedback/src/` for source surfaces, identity namespaces, planning policy, plan revisions, approval, and job contracts. Keep deterministic domain logic independent of storage, HTTP, and AI. Preserve deserialization of stored feedback and update `ARCHITECTURE.md` for new boundaries.
3. **Add integration and session storage.** Extend the backwards-compatible feedback document, with tests for tenant isolation, rotation, expiry, draft access, planning-policy changes, budget reservations, worker pairing, leases, idempotency, and concurrent writes. Persist credentials, sessions, idempotency, quotas, and jobs with the versioned feedback document so existing optimistic compare-and-swap saves commit them atomically. A future per-record storage migration must preserve those transaction boundaries.
4. **Build the feedback REST API.** Add API modules, routes in `monochange_app_api/src/lib.rs`, and rate-limit integration in `monochange_app/src/rate_limit.rs`. Reuse `feedback.rs` services and disclosure rules. Add session exchange, suggestions, drafts, explicit submissions, discussion, votes, and status.
5. **Build and package all three SDKs.** Add the web, React Native, and Flutter packages, script bundling, example hosts, and workspace build/lint/test wiring. Verify equivalent submission, suggestion, voting, discussion, and status behavior across all clients, along with lifecycle, accessibility, native layout, token refresh, account changes, retry handling, and preservation of drafts. Register new packages and their documentation in release configuration without publishing them.
6. **Add the assistant adapter.** Implement the selected model provider behind an application-layer interface. Add semantic retrieval, structured replies, budgets, timeouts, and a deterministic fallback. Test this phase with mocked providers; paid calls remain opt-in.
7. **Add connected-agent execution and plan review.** Add an application-layer worker protocol and local runner, with the exact CLI or crate location resolved against the existing architecture before implementation. Extend `pages/feedback_console.rs`, `server_fns/feedback.rs`, API authorization, plan/job persistence, and the repository adapter. Add worker pairing and revocation, manual and automatic planning, budget controls, revision review, approval/rejection, approval-bound implementation, and authenticated results. Document local-agent setup and update `docs/src/guide/feedback-agent-builds.md` to distinguish execution paths.
8. **Exercise the whole flow and document installation.** Add host-backend, website, React Native, and Flutter examples, widget and connected-worker guides under `docs/src/guide/`, and package/API documentation. Write separate developer and website changesets for the affected audiences, and first-release changesets for new published SDKs. Package publication is a human maintainer action.

Each phase should produce a reviewable increment. The complete feature is ready only when a request submitted from an external host reaches a reviewed plan, approval-bound implementation, and a visible delivery update.

## Acceptance and verification

- A signed-in host user can submit without GitHub login; web and mobile share a vote only when they share a configured identity namespace. Client-supplied ids cannot impersonate another subject.
- Web, React Native, and Flutter each provide the full initial feedback flow using the same API contracts; mobile SDKs do not depend on a WebView.
- An expired, forged, revoked, wrong-project, or wrong-surface token is rejected. Web origins are checked; native access does not weaken web validation. Quotas survive token refresh and expensive-job budgets survive restart.
- Similar visible requests appear before submission, including paraphrases when the provider is available. Upvoting does not create another item. Private items, plans, and other users' drafts never appear in suggestions or conversations.
- Resending a submission after a timeout creates one item. Retrying votes, plan approval, dispatch, and job callbacks does not duplicate votes, jobs, or pull requests.
- New text-only requests retain their source context, pass through triage, and appear in the correct project's console. Provider failures preserve the user's draft and explain how to proceed.
- Planning produces a complete reviewable revision. Rejection dispatches no implementation. Only a maintainer can approve; stale revisions and the old direct-build action cannot bypass approval.
- Manual planning is the default. Only maintainers can enable automatic feature planning, and only eligible feature requests queue automatically. Bugs, quarantined reports, duplicates, and incomplete briefs do not. Enabling does not plan the backlog; disabling stops new and unclaimed automatic work. Budget and concurrency limits hold under retries and races, and no automatic plan starts implementation.
- A worker can retrieve only its assigned brief and post results only for its job. Cancellation and late callbacks cannot restart work or overwrite a newer plan. Existing feedback documents and GitHub portal flows remain usable.
- A paired local or self-hosted agent completes planning and approved implementation without GitHub Actions. Revoked, unpaired, wrong-project, and stale-attempt workers cannot claim jobs or submit results. Offline jobs, worker loss, requested cancellation, and confirmed cancellation have distinct visible states. Workers remain supported on macOS, Linux, and Windows.
- An approved request reaches a draft pull request, review, merge, and the supported shipped update through existing observers.

Follow the repository's failing-test-first workflow. Rust unit tests belong in adjacent `__tests__/<module>_tests.rs` files; new integration tests belong in `crates/monochange_integration_tests`, use checked-in fixtures, and snapshot output with Insta. Redact multiline JSON fields and snapshot their text separately. Add meaningful web and React Native SDK tests, Dart transport tests, Flutter widget tests, and Android/iOS host verification for the native SDKs.

Run all repository commands through `devenv shell`. During implementation run the focused feedback, API, database, UI, and SDK tests, followed by `fix:all`, `build:all`, `lint:all`, `test:all`, `docs:check`, and `coverage:patch` with 100% coverage for executable changed lines. Run `monochange step validate`, `monochange step prepare-release --dry-run --format json`, and explicit-path affected-package verification; after committing code and changesets, run `monochange step affected-packages --from origin/main --verify` before opening a PR. Confirm the SDK packaging and website bundle budget checks when their artifacts change.

## Implementation record

- Domain: backwards-compatible integration/session state, private drafts, scoped identities, persistent quotas and idempotency, immutable planning briefs, explicit review history, budget reservations, leases, cancellation, confirmed-stop recovery, and retries.
- API: backend session exchange; visible suggestions; bounded structured conversation replies with deterministic fallback; draft/version guards; explicit submission, discussion, status, and idempotent votes; outbound worker pairing, claims, heartbeats, and results.
- Console: register/rotate/revoke apps; pair/revoke workers; select requests for planning; opt into automatic feature planning; inspect complete plans; approve, reject, request revisions, cancel, confirm stopped, and retry.
- Clients: installable web widget/headless client, native React Native sheet, native Flutter sheet, host-backend/web examples, React Native host component, and generated Android/iOS Flutter example projects.
- Execution: Node worker exports the pinned Git tree, keeps model credentials outside agent tools, enforces approved paths, runs operator-selected checks in an isolated container, saves results before posting, and reconciles provider results into one verified draft PR. Existing PR/release observers carry status through the supported delivery path.
- Release: SDK registrations, first-release changesets, separate application developer/product entries, generated browser bundle, build scripts, SDK CI, and installation/worker guides.

The storage decision deliberately reuses the existing versioned project document instead of introducing a second queue/database transaction. `ARCHITECTURE.md` and the installation guide describe its capacity limits. Provider calls remain outside retryable saves. A saved review decision survives superseding a revision so requested changes remain part of the next pinned brief.

No package publication, release workflow, production deployment, or merge is part of this implementation.

Native runtime verification is limited by this machine: device discovery twice returned “Could not communicate with device support. Try refreshing devices.” After clearing Nix SDK overrides, the iOS build finds Xcode but stops at its missing `CoreSimulator.framework` plugin; no Android SDK is installed. Flutter analyzer/unit/widget/example tests and the React Native component test run locally; the added CI workflow builds the Flutter Android/iOS hosts on supported runners. These platform build/runtime checks must be completed on a working native toolchain before shipping.

## Verification record

- Root Rust suite: 4,400 tests passed; documentation tests passed.
- Application workspace: 772 tests passed and one existing test remains ignored. This includes the complete approval/PR flow, stale briefs, cancellation, expiry, idempotency, quotas, origin/role isolation, and provider reconciliation.
- Node suite: 164 tests passed, including 19 feedback SDK/worker tests. The worker test exports real Git objects, excludes secret paths, and preserves executable metadata.
- Flutter: analyzer clean; four SDK tests and one example-host test passed. The Rust browser hydration target typechecks, and `cargo leptos build --release --split` builds both browser assets and the optimized server.
- CI follow-up: reproduced the fresh-checkout formatter failure without Dart package resolution. The Flutter task now resolves SDK and example dependencies before formatting; the same clean copy passes formatting, analysis, and all five tests.
- Cross-platform CI follow-up: snapshot filters retain the action endpoint name while normalizing its build-specific numeric hash. TypeScript resolves the feedback client from workspace source; type checking passes with its generated `dist` directory absent.
- iOS CI follow-up: native builds prioritize Apple tools and clear Nix SDK overrides so Flutter can discover the active Xcode. Native Xcode discovery succeeds locally; the full build reaches the separate missing simulator-framework installation described above.
- Agent-free CLI contracts: all 25 pass with zero model calls. Six expectations were updated to the current usage exit code and renderer messages, preserving the dry-run filesystem checks.
- Repository lint/build, application clippy with warnings denied, workspace validation, release dry-run/stream inspection, and explicit changed-path changeset verification pass.
- Combined root/application Rust coverage enforces 100% of executable changed lines. The separate app workspace is now included in `coverage:all`, preventing its API and UI code from falling outside the patch gate.
- Installable web, React Native, and worker npm tarballs and the Flutter source archive are in `.monochange/local/packages/`. Packed web exports and the worker executable passed smoke checks. The generated website widget also has a 32 KB raw / 10 KB gzip build budget.

The changes have not been published, deployed, or merged. Native runtime/build verification remains the platform limitation described above.

### Website bundle trade-off

The release build adds the integration controls and complete plan review to the existing lazy feedback-console route. Its compressed module grows from 165,937 to 303,067 bytes; total compressed site package contents grow from 1,312,748 to 1,447,392 bytes. Those totals include deferred route modules rather than an initial page transfer. The installable external-site widget remains a separate small JavaScript bundle and never loads the monochange app WASM. The measured split-build baseline is updated in `scripts/website-size-baseline.json`; the existing percentage tolerances stay in force for later changes.
