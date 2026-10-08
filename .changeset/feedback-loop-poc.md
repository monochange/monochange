---
monochange_app: patch
---

# Feedback loop proof of concept

Adds `monochange_app_feedback`, an in-process model of the loop between third-party app users and maintainers. Submissions carry page context and screenshot references, prompt-injection screening quarantines suspicious text until a maintainer approves it, a triage contract classifies reports and attempts reproduction behind the `TriageEngine` seam (a rule-based engine stands in for the AI agent), votes signal demand while acceptance stays maintainer-only with a recorded rationale required to override the threshold, and accepted items link GitHub issues and pull requests through a state machine that deliberately has no merge or push command. A disclosure gate renders all user-facing updates under the repository's visibility policy: private repositories get internal paths, stack frames, internal URLs, and secrets redacted, and lose issue/pull-request links, while release-notes links survive as published artifacts. Status feeds derive roadmap entries, vote counts, ship windows, and shipped lists from pipeline state. See `docs/plans/active/feedback-loop-poc.md` for the production wiring plan.
