use std::ffi::OsString;

use super::*;

fn args(values: &[&str]) -> Vec<OsString> {
	values.iter().map(OsString::from).collect()
}

fn environment(variables: &[(&str, &str)]) -> ColorEnvironment {
	ColorEnvironment::from_lookup(|name| {
		variables
			.iter()
			.find(|(key, _)| *key == name)
			.map(|(_, value)| (*value).to_string())
	})
}

#[test]
fn color_choice_parses_each_when_value() {
	assert_eq!(ColorChoice::parse("auto"), Some(ColorChoice::Auto));
	assert_eq!(ColorChoice::parse("always"), Some(ColorChoice::Always));
	assert_eq!(ColorChoice::parse("never"), Some(ColorChoice::Never));
	assert_eq!(ColorChoice::parse("sometimes"), None);
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

#[test]
fn color_environment_follows_no_color_and_clicolor_conventions() {
	assert_eq!(environment(&[]), ColorEnvironment::default());
	assert!(environment(&[("NO_COLOR", "1")]).no_color);
	assert!(!environment(&[("NO_COLOR", "")]).no_color);
	assert!(environment(&[("CLICOLOR_FORCE", "1")]).clicolor_force);
	assert!(!environment(&[("CLICOLOR_FORCE", "0")]).clicolor_force);
	assert!(environment(&[("CLICOLOR", "0")]).clicolor_disabled);
	assert!(!environment(&[("CLICOLOR", "1")]).clicolor_disabled);
	assert!(environment(&[("TERM", "dumb")]).term_is_dumb);
	assert!(!environment(&[("TERM", "xterm-256color")]).term_is_dumb);
}

#[test]
fn explicit_color_choices_override_the_environment() {
	let opted_out = environment(&[("NO_COLOR", "1"), ("TERM", "dumb")]);
	assert!(ColorChoice::Always.resolve(opted_out, false));
	let forced = environment(&[("CLICOLOR_FORCE", "1")]);
	assert!(!ColorChoice::Never.resolve(forced, true));
}

#[test]
fn auto_color_applies_environment_precedence_before_terminal_detection() {
	let auto = ColorChoice::Auto;
	assert!(auto.resolve(ColorEnvironment::default(), true));
	assert!(!auto.resolve(ColorEnvironment::default(), false));
	assert!(!auto.resolve(
		environment(&[("NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")]),
		true
	));
	assert!(auto.resolve(
		environment(&[("CLICOLOR_FORCE", "1"), ("TERM", "dumb")]),
		false
	));
	assert!(!auto.resolve(environment(&[("CLICOLOR", "0")]), true));
	assert!(!auto.resolve(environment(&[("TERM", "dumb")]), true));
}

#[test]
fn color_choice_is_scoped_to_the_running_command() {
	assert_eq!(color_choice(), ColorChoice::Auto);
	let scoped = crate::tests::block_on_in_context(with_color_choice(ColorChoice::Always, async {
		color_choice()
	}));
	assert_eq!(scoped, ColorChoice::Always);
	assert_eq!(color_choice(), ColorChoice::Auto);
}

#[test]
fn unit_tests_style_process_streams_only_when_asked() {
	assert!(!stdout_color());
	assert!(!stderr_color());
	let styled = crate::tests::block_on_in_context(with_color_choice(ColorChoice::Always, async {
		(stdout_color(), stderr_color())
	}));
	assert_eq!(styled, (true, true));
	let plain = crate::tests::block_on_in_context(with_color_choice(ColorChoice::Never, async {
		stdout_color()
	}));
	assert!(!plain);
}
