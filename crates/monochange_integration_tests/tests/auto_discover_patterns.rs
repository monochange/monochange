//! Invalid discovery filters must fail before an agent accepts incomplete ownership.

use std::path::Path;
use std::process::Command;

use monochange_test_helpers::get_cargo_bin;

#[test]
fn malformed_auto_discover_patterns_fail_the_cli() {
	for scenario in [
		"invalid-include",
		"invalid-exclude",
		"empty-include-invalid-exclude",
	] {
		let root = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../../fixtures/tests/config")
			.join(format!("auto-discover-{scenario}"));
		let output = Command::new(get_cargo_bin("monochange"))
			.current_dir(root)
			.env("NO_COLOR", "1")
			.env("MONOCHANGE_NO_PROGRESS", "1")
			.args(["config", "--format", "json"])
			.output()
			.unwrap_or_else(|error| panic!("run monochange config: {error}"));

		assert!(
			!output.status.success(),
			"{scenario} unexpectedly succeeded"
		);
		assert!(output.stdout.is_empty());
		let stderr = String::from_utf8(output.stderr)
			.unwrap_or_else(|error| panic!("config stderr must be UTF-8: {error}"));
		insta::assert_snapshot!(scenario, stderr);
	}
}
