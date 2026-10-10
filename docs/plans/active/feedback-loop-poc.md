# Project feedback

**Status**: in progress. The portal, console, persistence, and delivery webhooks are built; the widget, playground, and agents are next. **Related**: `docs/plans/active/monochange-platform-roadmap.md` (phase 3)

## What exists

- **Pipeline** (`app/crates/monochange_app_feedback`): intake validation and similarity, prompt-injection screening and quarantine, a `TriageEngine` seam with `RuleBasedTriage`, discussion and re-triage, voting, and maintainer-only decisions with recorded overrides. It also covers duplicate folding, the `GitHub` handoff drafts (issue, agent brief, changeset), merge and release observation by repository and number, and subscriber notifications. It has no storage, network, or AI code.
- **Disclosure**: `Sensitivity::allowed_on(Surface, RepositoryVisibility)` is the single rule for what text may say, on the portal and in the repository. A project follows its strictest repository, and a disconnected repository counts as private.
- **Persistence**: `project_feedback` holds one versioned `FeedbackState` document per project. `feedback_notifications` rows are written in the same transaction. `monochange_app_api::feedback::update` reruns an operation on a lost race, so operations must not have side effects outside the service. `GitHub` calls happen between updates.
- **Console** (`/dashboard/{org}/projects/{project}/feedback`): triage, what users see, held messages, and every allowed decision. It also creates the `GitHub` issue for accepted items in a chosen repository through the `GitHub` App. Every action is a plain form, so it works before hydration.
- **Portal** (`/p/{org}/{project}`): share a bug or idea in a Markdown editor with a toolbar and a server-rendered, sanitised preview. Similar requests are suggested before sending. Visitors can vote, discuss, answer follow-up questions, and read status updates. Visitors are pseudonymous: a random cookie becomes a per-project id.
- **Delivery webhooks**: a `pull_request` that closes an accepted item's issue (`Fixes #12`) or names it with a `Feedback-Item:` trailer moves the item to review. Merging it marks the item merged, and `release.published` ships every merged item in that repository; prereleases don't count. A trailer only applies when the item's issue lives in the pull request's repository, because item ids are per project and a repository can belong to several projects. Events that change nothing save nothing.

## Next

1. **Widget**: `/embed/feedback.js`, a shadow-DOM widget for apps (point at an element, screenshots), backed by REST endpoints with per-app origins and rate limits. This needs object storage for screenshots; until then the editor accepts text only.
2. **Playground** on `/feedback`: the earlier demo, as a per-visitor sandbox.
3. **Agents** (phase 4): the maintainer marks an item next, a `monochange/actions/agent` workflow plans in the repository's own CI, the owner approves, and progress is mirrored to the item and its issue.

## Known limits

- Screening and token classification are tripwires; the structural guards (reviewed titles, held messages, no merge rights) are the real protection.
- The `RuleBasedTriage` engine asks generic questions; the AI engine replaces it behind the same trait.
- Keyword similarity misses paraphrases.
- One document per project keeps writes simple and consistent; projects with thousands of items would want per-item rows.
- A release ships every merged item in its repository. That is right for releases cut from the default branch; a release from another branch would need the release's pull request list, which monochange release records carry (`observe_release`).
