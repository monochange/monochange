//! Command line entrypoint for the Acme inventory stack.

use acme_util::{format_quantity, parse_quantity, validate_batch};

fn main() {
	let args = std::env::args().skip(1).collect::<Vec<_>>();
	if args.is_empty() {
		eprintln!("usage: acme-tool parse <quantity> [...]");
		std::process::exit(2);
	}
	match args[0].as_str() {
		"parse" => {
			let parsed = args[1..]
				.iter()
				.filter_map(|text| parse_quantity(text))
				.collect::<Vec<_>>();
			for problem in validate_batch(&parsed) {
				eprintln!("warning: {problem}");
			}
			for (unit, amount) in parsed {
				println!("{}", format_quantity(unit, amount));
			}
		}
		other => {
			eprintln!("unknown command: {other}");
			std::process::exit(2);
		}
	}
}
