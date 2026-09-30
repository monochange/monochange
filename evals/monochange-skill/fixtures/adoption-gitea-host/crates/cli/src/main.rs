//! Command line entry point for the Acme rule engine.

use acme_engine::{Rule, matching_rules, rule_count};

fn main() {
	let rules = vec![
		Rule::new("user.create"),
		Rule::new("user.delete"),
		Rule::new("billing.invoice"),
	];

	let args = std::env::args().skip(1).collect::<Vec<_>>();
	let Some(query) = args.first() else {
		eprintln!("usage: acme-cli <query>");
		eprintln!("{} rules registered", rule_count(&rules));
		std::process::exit(2);
	};

	let matches = matching_rules(&rules, query);
	if matches.is_empty() {
		eprintln!("no rules matched {query:?}");
		std::process::exit(1);
	}

	for rule in matches {
		println!("{}", rule.key);
	}
}
