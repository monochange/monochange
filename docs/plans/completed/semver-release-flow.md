# Semantic SemVer release-flow guardrails

## Status

Completed and archived after `feat: add semantic semver release guardrails (#523)` landed.

## Goal

Use existing ecosystem semantic analyzers as release guardrails without replacing human-authored changesets. The first rollout is advisory: release planning keeps working, but release previews include compatibility evidence and warnings when semantic analysis suggests a stronger bump or finds uncovered package changes.

## Phases

1. **Normalize semantic impact**
   - Map `SemanticChange` records to a minimum `BumpSeverity`.
   - Aggregate per-package semantic changes into `CompatibilityAssessment` evidence.

2. **Wire evidence into release planning**
   - During release-plan construction, run semantic analysis for the detected git change frame.
   - Merge inferred compatibility evidence with existing changeset evidence before graph planning.
   - Keep analyzer failures advisory by adding release-plan warnings instead of failing the command.

3. **Surface release-preview guardrails**
   - Preserve `ReleasePlan.compatibility_evidence` in dry-run previews and release manifests.
   - Warn when semantic analysis sees package changes that do not have a matching pending changeset.

4. **Document and teach agents**
   - Document the advisory behavior in release-planning docs.
   - Update the monochange skill so agents inspect semantic evidence in release previews.

## Initial SemVer policy

- Removed or modified public API/export: `major`.
- Added public API/export: `minor`.
- Dependency or metadata changes: `patch` advisory evidence.
- Pre-1.0 version shifting remains handled by `BumpSeverity::apply_to_version` in the graph planner.

## Rollout notes

The guardrail is intentionally non-blocking. Future phases can add `[analysis.semver]` configuration for `warn`/`error` modes once analyzer confidence and repository adoption improve.
