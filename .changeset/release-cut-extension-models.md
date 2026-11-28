---
monochange_extension: patch
---

# Share the release-cut suggestion and setup models with the extension

`monochange_shared` now exports `ReleaseCutSuggestion` and `ReleaseCutSetupResult`. `ReleaseCutSuggestion` describes a connected repository whose recent merge velocity makes it a candidate for frozen release cuts, including its peak merged PRs in an hour, 24-hour throughput, burst summary, guide URL, and copyable agent prompt. `ReleaseCutSetupResult` reports the setup pull request opened or updated for that repository: its number, URL, branch, whether it was newly created, and whether monochange could commit the workflow file.

The extension compiles against `monochange_shared`, so these types are part of its shared contract and are available for future release-cut UI. This is shared model support only: the popup, options page, and background worker render and behave exactly as before, no extension code consumes the models yet, and existing installations need no action.
