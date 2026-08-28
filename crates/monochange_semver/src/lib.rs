#![doc(
	html_logo_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-512.png",
	html_favicon_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/favicon.ico"
)]
#![forbid(clippy::indexing_slicing)]

//! # `monochange_semver`
//!
//! `monochange_semver` merges requested bumps with compatibility evidence.
//!
//! Reach for this crate when you need deterministic severity calculations for direct changes, propagated dependent changes, or ecosystem-specific compatibility providers.
//!
//! ## Why use it?
//!
//! - combine manual change requests with provider-generated compatibility assessments
//! - share one bump-merging strategy across the workspace
//! - implement custom `CompatibilityProvider` integrations for ecosystem-specific evidence
//!
//! ## Best for
//!
//! - computing release severities outside the full planner
//! - plugging ecosystem-specific compatibility logic into shared planning
//! - reusing the workspace's bump-merging rules in custom tools
//!
//! ## Responsibilities
//!
//! - collect compatibility assessments from providers
//! - merge bump severities deterministically
//! - calculate direct and propagated bump severities
//! - provide a shared abstraction for ecosystem-specific compatibility providers
//!
//! ## Example
//!
//! ```rust
//! use monochange_core::BumpSeverity;
//! use monochange_semver::direct_release_severity;
//! use monochange_semver::merge_severities;
//!
//! let merged = merge_severities(BumpSeverity::Patch, BumpSeverity::Minor);
//! let direct = direct_release_severity(Some(BumpSeverity::Minor), None);
//!
//! assert_eq!(merged, BumpSeverity::Minor);
//! assert_eq!(direct, BumpSeverity::Minor);
//! ```
use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::CompatibilityAssessment;
use monochange_core::PackageRecord;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;

/// Provider interface for ecosystem-specific compatibility evidence.
pub trait CompatibilityProvider {
	fn provider_id(&self) -> &'static str;

	fn assess(
		&self,
		package: &PackageRecord,
		change_signal: &ChangeSignal,
	) -> Option<CompatibilityAssessment>;
}

/// Collect compatibility assessments for the supplied change signals.
#[must_use]
pub fn collect_assessments(
	providers: &[&dyn CompatibilityProvider],
	packages: &[PackageRecord],
	change_signals: &[ChangeSignal],
) -> Vec<CompatibilityAssessment> {
	change_signals
		.iter()
		.filter_map(|change_signal| {
			packages
				.iter()
				.find(|package| package.id == change_signal.package_id)
				.map(|package| (package, change_signal))
		})
		.flat_map(|(package, change_signal)| {
			providers
				.iter()
				.filter_map(|provider| provider.assess(package, change_signal))
		})
		.collect()
}

/// Merge two bump severities and return the higher one.
#[must_use]
pub fn merge_severities(left: BumpSeverity, right: BumpSeverity) -> BumpSeverity {
	left.max(right)
}

/// Return the strongest assessment from a list.
#[must_use]
pub fn strongest_assessment(
	assessments: &[CompatibilityAssessment],
) -> Option<CompatibilityAssessment> {
	assessments
		.iter()
		.cloned()
		.max_by_key(|assessment| assessment.severity)
}

/// Return the strongest assessment for a specific package.
#[must_use]
pub fn strongest_assessment_for_package(
	assessments: &[CompatibilityAssessment],
	package_id: &str,
) -> Option<CompatibilityAssessment> {
	let matching = assessments
		.iter()
		.filter(|assessment| assessment.package_id == package_id)
		.cloned()
		.collect::<Vec<_>>();

	strongest_assessment(&matching)
}

/// Calculate the effective direct-release severity for a package.
#[must_use]
pub fn direct_release_severity(
	requested_bump: Option<BumpSeverity>,
	assessment: Option<&CompatibilityAssessment>,
) -> BumpSeverity {
	merge_severities(
		requested_bump.unwrap_or(BumpSeverity::Patch),
		assessment.map_or(BumpSeverity::None, |value| value.severity),
	)
}

/// Calculate the propagated severity applied to dependents of a changed package.
#[must_use]
pub fn propagated_release_severity(
	default_parent_bump: BumpSeverity,
	assessment: Option<&CompatibilityAssessment>,
) -> BumpSeverity {
	merge_severities(
		default_parent_bump,
		assessment.map_or(BumpSeverity::None, |value| value.severity),
	)
}

/// Infer the minimum `SemVer` bump for one semantic diff record.
#[must_use]
#[rustfmt::skip]
pub fn semantic_change_severity(change: &SemanticChange) -> BumpSeverity {
	if matches!(
		(change.category, change.kind),
		(
			SemanticChangeCategory::PublicApi | SemanticChangeCategory::Export,
			SemanticChangeKind::Removed | SemanticChangeKind::Modified
		)
	) {
		return BumpSeverity::Major;
	}

	if matches!(
		(change.category, change.kind),
		(
			SemanticChangeCategory::PublicApi | SemanticChangeCategory::Export,
			SemanticChangeKind::Added
		)
	) {
		return BumpSeverity::Minor;
	}

	let is_dependency_or_metadata = matches!(
		change.category,
		SemanticChangeCategory::Dependency | SemanticChangeCategory::Metadata
	);
	if is_dependency_or_metadata { BumpSeverity::Patch } else { BumpSeverity::None }
}
/// Build release-planning compatibility evidence from semantic analyzer output.
#[must_use]
pub fn semantic_changes_assessment(
	package_id: &str,
	provider_id: &str,
	semantic_changes: &[SemanticChange],
) -> Option<CompatibilityAssessment> {
	let severity = semantic_changes
		.iter()
		.map(semantic_change_severity)
		.max()
		.unwrap_or(BumpSeverity::None);

	if !severity.is_release() {
		return None;
	}

	let first_change = semantic_changes
		.first()
		.expect("release-impact semantic assessments include at least one semantic change");
	let evidence_location = Some(first_change.file_path.display().to_string());
	let example = first_change.summary.clone();

	Some(CompatibilityAssessment {
		package_id: package_id.to_string(),
		provider_id: provider_id.to_string(),
		severity,
		confidence: semantic_assessment_confidence(severity).to_string(),
		summary: format!(
			"{severity} SemVer impact inferred from {} semantic change(s); {example}",
			semantic_changes.len()
		),
		evidence_location,
	})
}

fn semantic_assessment_confidence(severity: BumpSeverity) -> &'static str {
	match severity {
		BumpSeverity::Major | BumpSeverity::Minor => "high",
		BumpSeverity::Patch => "medium",
		_ => "low",
	}
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
