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
	assert!(tally.has_voted("u1"));
	assert!(!tally.has_voted("u2"));
	assert_eq!(tally.total(), 1);
}

#[test]
fn total_counts_unique_submitters() {
	let mut tally = VoteTally::default();
	for submitter in ["u1", "u2", "u3"] {
		tally.record(submitter);
		tally.record(submitter);
	}
	assert_eq!(tally.total(), 3);
}

#[test]
fn evaluate_reports_threshold() {
	let rules = VotingRules::default();
	let mut tally = VoteTally::default();
	tally.record("u1");
	tally.record("u2");
	assert_eq!(
		evaluate(&tally, &rules),
		VotingOutcome::BelowThreshold { votes: 2 }
	);
	tally.record("u3");
	assert_eq!(
		evaluate(&tally, &rules),
		VotingOutcome::ThresholdReached { votes: 3 }
	);
	let single_vote_rules = VotingRules {
		acceptance_threshold: 1,
	};
	let mut single = VoteTally::default();
	single.record("u1");
	assert_eq!(
		evaluate(&single, &single_vote_rules),
		VotingOutcome::ThresholdReached { votes: 1 }
	);
}
