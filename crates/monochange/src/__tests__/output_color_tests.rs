use std::ffi::OsString;

use super::*;

fn args(values: &[&str]) -> Vec<OsString> {
	values.iter().map(OsString::from).collect()
}

#[test]
fn color_choice_reads_both_flag_spellings_and_keeps_the_last_valid_value() {
	assert_eq!(
		ColorChoice::from_args(&args(&["mc", "next"])),
		ColorChoice::Auto
	);
	assert_eq!(
		ColorChoice::from_args(&args(&["mc", "--color", "always", "next"])),
		ColorChoice::Always
	);
	assert_eq!(
		ColorChoice::from_args(&args(&["mc", "--color=never", "next"])),
		ColorChoice::Never
	);
	assert_eq!(
		ColorChoice::from_args(&args(&["mc", "--color=always", "--color", "bogus"])),
		ColorChoice::Always
	);
	assert_eq!(
		ColorChoice::from_args(&args(&["mc", "--color"])),
		ColorChoice::Auto
	);
}
