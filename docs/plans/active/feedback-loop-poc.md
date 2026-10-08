# Feedback loop proof of concept

**Status**: in progress (POC crate landed, production wiring not started) **Branch**: `feat/feedback-loop` **Related**: `docs/plans/active/monochange-app-planning.md` (phases 2–3 reserve the UI, database, and widget surfaces this pipeline feeds)

## Problem

monochange plans releases but has no loop back from the people using the software: third-party app users cannot report bugs or request features from inside the app, there is no triage or voting step, accepted work is not pipelined into issues and pull requests, and users never learn what shipped or when their request is likely to ship. The hard constraint is privacy: feedback portals for private repositories must never expose codebase internals, while public repositories can share links and technical detail freely.

## What exists after this POC

`app/crates/monochange_app_feedback` models the whole loop in-process, with no network, database, or AI client code:

- **Intake** (`submission.rs`): `FeedbackSubmission` carries a description, optional page context (route, app version), screenshot/recording attachments as opaque media ids, a pseudonymous submitter, and the registered app slug that submitted it.
- **Screening** (`triage.rs`): `screen_untrusted` flags prompt-injection markers; suspect items are quarantined and only a maintainer can resume triage.
- **Triage contract** (`triage.rs`): `TriageEngine` returns classification, a reproduction outcome, follow-up questions, sensitivity-tagged findings for maintainers, and a product-language summary. `RuleBasedTriage` is the deterministic stand-in for the AI agent; untrusted descriptions get a neutral summary so they are never echoed to other users.
- **Pipeline** (`pipeline.rs`): a stage machine (Received → Quarantined → Triaging → Discussing → Voting → Accepted/Declined → Building → InReview → Shipped/Closed) that encodes authority: automated actors triage and record artifacts, only maintainers accept, decline, or close, accepting below the vote threshold requires a recorded override rationale, and there is no merge or push command at all — releases are observed via `MarkShipped`.
- **Voting** (`voting.rs`): one vote per anonymous submitter; votes signal demand and never accept work on their own.
- **Disclosure gate** (`disclosure.rs`): `DisclosurePolicy::for_visibility` plus `DisclosureGate::publish` decide what leaves the maintainers' side. Private repositories get redaction of internal paths, stack frames, internal URLs, and secrets; issue/PR links and technical detail are dropped; release notes links always stay (they are published artifacts). Every outbound update is re-screened for untrusted instructions regardless of visibility.
- **Status feedback** (`roadmap.rs`): `status_feed` builds the public roadmap (UnderReview/Planned/InProgress/Declined, votes, ship windows from the release cadence) and the shipped list (version + release-notes link). `public_item_update` is the per-item widget view.

## Non-goals of the POC

- No HTTP endpoints, database models, or widget — the existing stub server functions (`app/crates/monochange_app/src/server_fns/feedback.rs`, `roadmap.rs`) stay untouched.
- No AI client; `TriageEngine` is the seam where the OpenRouter-backed agent from the app plan plugs in.
- No GitHub calls; the app's existing `github_app` client creates the issue and pull request and feeds the refs back through service methods.

## Production wiring (ordered)

1. Database: `feedback_forms`, `feedback_submissions`, `feedback_votes`, `roadmap_items` welds models + migration in `monochange_app_db`, mapping the crate's stages to the `stage` string column and events to a JSON log.
2. API: REST endpoints behind `monochange_app_api::api_router` mirroring the service methods (`submit`, `vote`, `status`, per-app widget config), plus widget CORS from `feedback_forms.allowed_domains`.
3. Widget: `embed/` TypeScript bundle capturing page context, screenshot, and description; it only ever talks to the REST endpoints.
4. AI triage: implement `TriageEngine` on top of the app's model client; bug reproduction runs the agent against a checkout of the repository with a tool allow-list. Quarantine plus maintainer approval stays the gate for anything screening flags.
5. GitHub: on accept, create the issue from the maintainer-side draft (full triage detail is fine there — it lives inside the repository); on PR open and release, feed refs back and let webhooks drive `MarkShipped` from the release automation.
6. Roadmap/changelog pages: public routes `/:owner/:repo/{roadmap,feedback}` render `status_feed`; the existing `website_json` release output already provides the shipped entries.

## Privacy decisions to keep

- Disclosure direction is outward: maintainer artifacts may contain internals because they live inside the repository; user-facing artifacts pass the gate. Public repositories pass through; private ones are redacted and lose issue/PR links.
- Titles truncate at word boundaries so a char-boundary cut cannot split a path token out from behind the redactor.
- AI summaries of quarantined descriptions are neutral until a maintainer rewrites them; the gate re-screens all outbound text so a hostile or buggy engine cannot smuggle instructions to other users.

## Validation

```bash
devenv shell -- bash -c 'cd app && cargo test -p monochange_app_feedback'
devenv shell -- bash -c 'cd app && cargo clippy -p monochange_app_feedback --all-targets'
```

58 tests, clippy clean under the workspace pedantic set. Line coverage is 671/672 in the crate; the single missed line is a derive-generated function in `roadmap.rs`, not a hand-written path. The repo's `coverage:patch` task measures the main workspace only, so wiring the app workspace into patch coverage is follow-up work.

## Follow-up risks

- Screening markers are conservative tripwires, not a defense; the real protection is that untrusted text is never echoed and agents never run with write credentials from user input alone.
- Vote brigading: `anonymous_id` dedupes but does not rate-limit; the API layer needs per-app throttling.
- Ship windows are only as honest as the cadence snapshot the automation feeds in; keep labels qualitative until scheduling is trustworthy.
