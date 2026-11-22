use monochange_core::BumpSeverity;
use monochange_core::CompatibilityAssessment;

use crate::direct_release_severity;
use crate::merge_severities;
use crate::propagated_release_severity;
use crate::strongest_assessment;

fn make_assessment(package_id: &str, severity: BumpSeverity) -> CompatibilityAssessment {
	CompatibilityAssessment {
		package_id: package_id.to_string(),
		provider_id: "test".to_string(),
		severity,
		confidence: "high".to_string(),
		summary: format!("{severity:?} change"),
		evidence_location: None,
	}
}

// -- property tests --
use proptest::prelude::*;
use proptest::prop_compose;
use proptest::proptest;

prop_compose! {
	fn arbitrary_bump_severity()(n in 0..4u8) -> BumpSeverity {
		match n {
			0 => BumpSeverity::None,
			1 => BumpSeverity::Patch,
			2 => BumpSeverity::Minor,
			3 => BumpSeverity::Major,
			_ => unreachable!(),
		}
	}
}

proptest! {
	#[test]
	fn merge_severities_is_commutative_proptest(
		left in arbitrary_bump_severity(),
		right in arbitrary_bump_severity()
	) {
		prop_assert_eq!(
			merge_severities(left, right),
			merge_severities(right, left)
		);
	}

	#[test]
	fn merge_severities_is_associative_proptest(
		a in arbitrary_bump_severity(),
		b in arbitrary_bump_severity(),
		c in arbitrary_bump_severity()
	) {
		prop_assert_eq!(
			merge_severities(merge_severities(a, b), c),
			merge_severities(a, merge_severities(b, c))
		);
	}

	#[test]
	fn merge_severities_with_none_is_identity_proptest(
		severity in arbitrary_bump_severity()
	) {
		prop_assert_eq!(
			merge_severities(severity, BumpSeverity::None),
			severity
		);
		prop_assert_eq!(
			merge_severities(BumpSeverity::None, severity),
			severity
		);
	}

	#[test]
	fn merge_severities_idempotence_proptest(
		x in arbitrary_bump_severity()
	) {
		prop_assert_eq!(
			merge_severities(x, x),
			x
		);
	}

	#[test]
	fn merge_severities_with_major_is_absorbing_proptest(
		x in arbitrary_bump_severity()
	) {
		prop_assert_eq!(
			merge_severities(BumpSeverity::Major, x),
			BumpSeverity::Major
		);
		prop_assert_eq!(
			merge_severities(x, BumpSeverity::Major),
			BumpSeverity::Major
		);
	}

	#[test]
	fn merge_severities_result_is_gte_each_input_proptest(
		left in arbitrary_bump_severity(),
		right in arbitrary_bump_severity()
	) {
		let merged = merge_severities(left, right);
		prop_assert!(
			merged >= left,
			"merge({left:?}, {right:?}) = {merged:?} < {left:?}"
		);
		prop_assert!(
			merged >= right,
			"merge({left:?}, {right:?}) = {merged:?} < {right:?}"
		);
	}

	#[test]
	fn strongest_assessment_returns_highest_severity_proptest(
		assessments in proptest::collection::vec(
			(arbitrary_bump_severity(), "[a-z]{1,16}"),
			0..20
		)
	) {
		let inputs: Vec<CompatibilityAssessment> = assessments
			.iter()
			.map(|(severity, package_id)| {
				make_assessment(package_id, *severity)
			})
			.collect();

		let strongest = strongest_assessment(&inputs);

		if let Some(ref strongest) = strongest {
			for assessment in &inputs {
				prop_assert!(
					strongest.severity >= assessment.severity,
					"strongest {strongest:?} is weaker than {assessment:?}"
				);
			}
		} else {
			prop_assert!(
				inputs.is_empty(),
				"non-empty input produced None for strongest"
			);
		}
	}

	#[test]
	fn strongest_assessment_max_severity_equals_max_of_inputs_proptest(
		assessments in proptest::collection::vec(
			(arbitrary_bump_severity(), "[a-z]{1,16}"),
			1..20
		)
	) {
		let inputs: Vec<CompatibilityAssessment> = assessments
			.iter()
			.map(|(severity, package_id)| {
				make_assessment(package_id, *severity)
			})
			.collect();

		let max_severity = inputs.iter().map(|a| a.severity).max().unwrap();
		let strongest = strongest_assessment(&inputs).unwrap();
		prop_assert_eq!(
			strongest.severity,
			max_severity,
			"strongest severity {:?} does not equal max {:?}",
			strongest.severity,
			max_severity
		);
	}

	#[test]
	fn direct_release_severity_never_less_than_requested_proptest(
		requested in arbitrary_bump_severity(),
		assessment in arbitrary_bump_severity()
	) {
		let assessment = make_assessment("core", assessment);
		let result = direct_release_severity(Some(requested), Some(&assessment));
		prop_assert!(
			result >= requested,
			"direct_release_severity({requested:?}, Some({assessment:?})) = {result:?}"
		);
		prop_assert!(
			result >= assessment.severity,
			"direct_release_severity({requested:?}, Some({assessment:?})) = {result:?}"
		);
	}

	#[test]
	fn propagated_release_severity_never_less_than_parent_proptest(
		parent in arbitrary_bump_severity(),
		assessment in arbitrary_bump_severity()
	) {
		let assessment = make_assessment("core", assessment);
		let result = propagated_release_severity(parent, None, BumpSeverity::Patch, Some(&assessment));
		prop_assert!(
			result >= parent,
			"propagated_release_severity({parent:?}, Some({assessment:?})) = {result:?}"
		);
		prop_assert!(
			result >= assessment.severity,
			"propagated_release_severity({parent:?}, Some({assessment:?})) = {result:?}"
		);
	}
}
