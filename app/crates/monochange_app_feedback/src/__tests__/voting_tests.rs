use crate::voting::VoteTally;
use crate::voting::VotingOutcome;
use crate::voting::VotingRules;
use crate::voting::evaluate;

#[test]
fn default_threshold_is_three() {
	assert_eq!(VotingRules::default().acceptance_threshold, 3);
}

#[test]
fn records_one_vote_per_submitter() {
	let mut tally = VoteTally::default();
	assert!(tally.record("u1"));
	assert!(!tally.record("u1"));
	assert!(tally.record("u2"));
	assert!(tally.has_voted("u1"));
	assert!(!tally.has_voted("u3"));
	assert_eq!(tally.total(), 2);
	assert_eq!(tally.voters().collect::<Vec<_>>(), ["u1", "u2"]);
}

#[test]
fn retracting_removes_only_existing_votes() {
	let mut tally = VoteTally::default();
	tally.record("u1");
	assert!(tally.retract("u1"));
	assert!(!tally.retract("u1"));
	assert_eq!(tally.total(), 0);
}

#[test]
fn evaluation_compares_against_the_threshold() {
	let rules = VotingRules {
		acceptance_threshold: 2,
	};
	let mut tally = VoteTally::default();
	tally.record("u1");
	assert_eq!(
		evaluate(&tally, &rules),
		VotingOutcome::BelowThreshold { votes: 1 }
	);
	tally.record("u2");
	assert_eq!(
		evaluate(&tally, &rules),
		VotingOutcome::ThresholdReached { votes: 2 }
	);
}

#[test]
fn voting_types_round_trip_through_json() {
	let mut tally = VoteTally::default();
	tally.record("u1");
	let decoded: VoteTally = serde_json::from_value(serde_json::to_value(&tally).unwrap()).unwrap();
	assert_eq!(decoded, tally);
	let rules: VotingRules =
		serde_json::from_value(serde_json::to_value(VotingRules::default()).unwrap()).unwrap();
	assert_eq!(rules, VotingRules::default());
	let outcome = VotingOutcome::ThresholdReached { votes: 3 };
	let json = serde_json::to_value(outcome).unwrap();
	assert_eq!(json["outcome"], "threshold_reached");
	assert_eq!(
		serde_json::from_value::<VotingOutcome>(json).unwrap(),
		outcome
	);
}
