//! Command line entrypoint for the Acme toolchain.

use acme_core::{Document, validate};

fn main() {
	let args = std::env::args().skip(1).collect::<Vec<_>>();
	if args.is_empty() {
		eprintln!("usage: acme-cli <command> [args]");
		std::process::exit(2);
	}
	match args[0].as_str() {
		"check" => {
			let Some(path) = args.get(1) else {
				eprintln!("check requires a path");
				std::process::exit(2);
			};
			let source = std::fs::read_to_string(path).expect("read input file");
			let document = Document::parse(&source);
			let problems = validate(&document);
			if problems.is_empty() {
				println!("ok: {} lines", document.line_count());
			} else {
				for problem in problems {
					eprintln!("error: {problem}");
				}
				std::process::exit(1);
			}
		}
		other => {
			eprintln!("unknown command: {other}");
			std::process::exit(2);
		}
	}
}
