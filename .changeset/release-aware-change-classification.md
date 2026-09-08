---
"@monochange/cli": minor
"@monochange/skill": minor
monochange: minor
monochange_analysis: minor
monochange_core: minor
monochange_ecmascript: patch
---

# classify pull request severity against both the default branch and latest release

Agents and reviewers can now run one command to propose a `major`, `minor`, `patch`, or `none` changeset for every affected package:

```bash
monochange change classify --format json --dependency-propagation public
```

The versioned JSON report resolves the remote default branch, models the pull request's merge result, finds each release owner's latest reachable tag, and keeps the current pull request recommendation separate from the accumulated release floor. Each package reports compatibility impact, the proposed bump, the enforceable high-confidence minimum, confidence, completeness, pending changesets, the required changeset action, and stable finding ids. Findings include their analyzer, source location, before and after signatures, and the comparisons in which they occur.

Markdown output contains the same decision evidence for terminal output, job summaries, and pull request comments. The `monochange_classify_changes` MCP tool returns the JSON contract and accepts the same base, head, release, package, and detection controls.

Changeset validation now enforces only high-confidence findings by default:

```bash
monochange changeset validate --api --format markdown
```

Use `--strict` to require pending changesets to satisfy partial or medium-confidence proposals too. A changed package that has no modeled semantic finding now receives a low-confidence patch proposal and a review requirement instead of a false `none` result.

`monochange_analysis::AnalysisSession` is available for tools that compare several git frames. It discovers the package graph once and reuses it:

```rust
use monochange_analysis::{AnalysisConfig, AnalysisSession, ChangeFrame};

let session = AnalysisSession::new(root, AnalysisConfig::default())?;
let pull_request = session.analyze(&ChangeFrame::CustomRange {
	base: "origin/main".into(),
	head: "HEAD".into(),
})?;
```

Exact custom ranges now use `base..head`, while pull request source deltas retain merge-base semantics. Working-directory analysis includes staged, unstaged, deleted, and untracked files. Git revision snapshots use batched object reads, which avoids starting one Git process per file.

`monochange_core::PackagePathMatcher` provides the shared package path policy used by changeset coverage and semantic analysis. Classification honors configured package boundaries, package-level additional and ignored paths, and workspace-level changeset ignores:

```rust
use monochange_core::{PackagePathMatch, PackagePathMatcher};

let matcher = PackagePathMatcher::new(
	"web",
	"packages/web".as_ref(),
	&["shared/schema/**".into()],
	&["fixtures/**".into()],
);
assert_eq!(
	matcher.classify("shared/schema/api.json".as_ref()),
	PackagePathMatch::Touched,
);
```

TypeScript and JavaScript function signatures now preserve object-shaped parameter and return types while excluding implementation bodies, so changes inside those public types are no longer hidden from classification.
