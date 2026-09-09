---
"@monochange/cli": minor
"@monochange/skill": minor
monochange: minor
monochange_analysis: breaking
monochange_cargo: patch
monochange_core: breaking
monochange_dart: patch
monochange_deno: patch
monochange_ecmascript: patch
monochange_npm: minor
monochange_semver: minor
---

# classify TypeScript declaration compatibility with the project compiler

`monochange change classify --detection-level semantic` now resolves each npm package's typed entrypoints, emits isolated before and after declaration surfaces, and asks the workspace's TypeScript compiler whether the consumer contract is breaking, additive, compatible, or inconclusive. Complete breaking evidence proposes `major`, additive evidence proposes `minor`, and compatible implementation-only changes can propose `none`.

Install `node`, TypeScript, and the package dependencies before classification. Missing tools, invalid configuration, unresolved dependencies, wildcard exports, and identity-sensitive generic or nominal types remain visible as partial evidence with a conservative `patch` proposal and `reviewRequired: true`.

Findings now include the semantic engine, exact engine version, coverage completeness, coverage note, and fallback reason in JSON, text, Markdown, MCP output, job summaries, and pull request comments. The change-classification JSON schema version is now `2`.

The `monochange_core::SemanticChange` struct is now non-exhaustive and has an optional `assessment`. Custom analyzers should construct findings with `SemanticChange::new` and attach compiler evidence with `with_assessment` instead of using a struct literal:

```rust
let change = SemanticChange::new(
	SemanticChangeCategory::PublicApi,
	SemanticChangeKind::Modified,
	"function",
	"parse",
	"function `parse` changed",
	"src/lib.rs",
)
.with_assessment(SemanticChangeAssessment::new(
	SemanticAnalysisOutcome::Breaking,
	BumpSeverity::Major,
	ApiConfidence::High,
	SemanticAnalyzerEvidence::new(
		"example/analyzer",
		"example-engine",
		SemanticAnalysisCompleteness::Complete,
		"all public entrypoints checked",
	),
));
```

The semver layer clamps analyzer recommendations to the minimum severity implied by their outcome, so malformed or third-party evidence cannot understate a proven breaking or additive change.
