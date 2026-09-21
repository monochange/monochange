#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use crate::versioning::CounterFileError;
use crate::versioning::GitSource;
use crate::versioning::HashEncoding;
use crate::versioning::LabelInputs;
use crate::versioning::ReleaseTimestamp;
use crate::versioning::ResetPolicy;
use crate::versioning::StampBehaviour;
use crate::versioning::TimestampSource;
use crate::versioning::ValueDefinition;
use crate::versioning::chain_label_inputs;
use crate::versioning::counter_field_segments;
use crate::versioning::counter_from_json;
use crate::versioning::encode_hash;
use crate::versioning::render_version_template;
use crate::versioning::template_variables;
use crate::versioning::template_variables_used;
use crate::versioning::validate_template_variables;
use crate::versioning::validate_value_definition;
use crate::versioning::validate_value_id;
use crate::versioning::value_is_monotonic;

fn timestamp(year: u16, month: u8, day: u8) -> ReleaseTimestamp {
	ReleaseTimestamp::new(year, month, day, 12, 30, 45)
		.unwrap_or_else(|error| panic!("valid timestamp: {error}"))
}

fn value_definition(toml_source: &str) -> ValueDefinition {
	toml::from_str(toml_source)
		.unwrap_or_else(|error| panic!("value definition should parse: {error}"))
}

#[test]
fn stamp_behaviour_increments_from_zero() {
	assert_eq!(StampBehaviour::Increment.apply(0), 1);
	assert_eq!(StampBehaviour::Increment.apply(41), 42);
}

#[test]
fn stamp_behaviour_adds_configured_amount() {
	let behaviour = StampBehaviour::Add { amount: 10 };
	assert_eq!(behaviour.apply(0), 10);
	assert_eq!(behaviour.apply(41), 51);
}

#[test]
fn stamp_behaviour_none_is_read_only() {
	assert_eq!(StampBehaviour::None.apply(7), 7);
	assert!(!StampBehaviour::None.is_stamped());
}

#[test]
fn stamp_behaviour_increment_saturates_instead_of_wrapping() {
	assert_eq!(StampBehaviour::Increment.apply(u64::MAX), u64::MAX);
	assert_eq!(StampBehaviour::Add { amount: 10 }.apply(u64::MAX), u64::MAX);
}

#[test]
fn stamp_behaviour_strings_match_config_values() {
	assert_eq!(StampBehaviour::Increment.as_str(), "increment");
	assert_eq!(StampBehaviour::Add { amount: 1 }.as_str(), "add");
	assert_eq!(StampBehaviour::None.as_str(), "none");
	assert_eq!(ResetPolicy::Never.as_str(), "never");
	assert_eq!(ResetPolicy::Version.as_str(), "version");
	assert_eq!(TimestampSource::Now.as_str(), "now");
	assert_eq!(TimestampSource::Commit.as_str(), "commit");
	assert_eq!(GitSource::ShortHash.as_str(), "short_hash");
	assert_eq!(GitSource::CommitCount.as_str(), "commit_count");
	assert_eq!(HashEncoding::Hex.as_str(), "hex");
	assert_eq!(HashEncoding::Base32.as_str(), "base32");
	assert_eq!(HashEncoding::Base36.as_str(), "base36");
	assert_eq!(HashEncoding::Digits.as_str(), "digits");
}

#[test]
fn release_timestamp_rejects_out_of_range_components() {
	assert!(ReleaseTimestamp::new(2026, 13, 1, 0, 0, 0).is_err());
	assert!(ReleaseTimestamp::new(2026, 1, 32, 0, 0, 0).is_err());
	assert!(ReleaseTimestamp::new(2026, 1, 1, 24, 0, 0).is_err());
	assert!(ReleaseTimestamp::new(2026, 1, 1, 0, 60, 0).is_err());
	assert!(ReleaseTimestamp::new(2026, 1, 1, 0, 0, 60).is_err());
}

#[test]
fn release_timestamp_parses_and_renders() {
	let parsed = ReleaseTimestamp::parse("2026-09-19", "153045")
		.unwrap_or_else(|error| panic!("valid: {error}"));
	assert_eq!(parsed.date(), "2026-09-19");
	assert_eq!(parsed.time(), "153045");
	assert_eq!(parsed.date(), "2026-09-19");
	assert_eq!(parsed.date_compact(), "20260919");
	assert_eq!(parsed.time(), "153045");
	assert_eq!(parsed.month_padded(), "09");
	assert_eq!(parsed.year_short(), 26);
}

#[test]
fn release_timestamp_rejects_malformed_input() {
	assert!(ReleaseTimestamp::parse("2026-09", "153045").is_err());
	assert!(ReleaseTimestamp::parse("not-a-date", "153045").is_err());
	assert!(ReleaseTimestamp::parse("2026-09-19-01", "153045").is_err());
	assert!(ReleaseTimestamp::parse("2026-09-19", "1530").is_err());
	assert!(ReleaseTimestamp::parse("2026-09-19", "15z045").is_err());
}

#[test]
fn release_timestamp_derives_quarters() {
	assert_eq!(timestamp(2026, 1, 1).quarter(), 1);
	assert_eq!(timestamp(2026, 3, 31).quarter(), 1);
	assert_eq!(timestamp(2026, 4, 1).quarter(), 2);
	assert_eq!(timestamp(2026, 9, 19).quarter(), 3);
	assert_eq!(timestamp(2026, 12, 31).quarter(), 4);
}

#[test]
fn label_inputs_chain_within_the_same_month() {
	let now = timestamp(2026, 9, 19);
	let previous = LabelInputs {
		date: "2026-09-01".to_string(),
		time: "000000".to_string(),
		of_month: 1,
		of_quarter: 1,
		of_year: 4,
	};
	let chained = chain_label_inputs(Some(&previous), now);
	assert_eq!(chained.of_month, 2);
	assert_eq!(chained.of_quarter, 2);
	assert_eq!(chained.of_year, 5);
	assert_eq!(chained.date, "2026-09-19");
}

#[test]
fn label_inputs_reset_month_but_keep_quarter_and_year() {
	let now = timestamp(2026, 9, 19);
	let previous = LabelInputs {
		date: "2026-08-31".to_string(),
		time: "000000".to_string(),
		of_month: 6,
		of_quarter: 2,
		of_year: 7,
	};
	let chained = chain_label_inputs(Some(&previous), now);
	assert_eq!(chained.of_month, 1);
	assert_eq!(chained.of_quarter, 3);
	assert_eq!(chained.of_year, 8);
}

#[test]
fn label_inputs_reset_quarter_but_keep_year() {
	let now = timestamp(2026, 10, 1);
	let previous = LabelInputs {
		date: "2026-09-30".to_string(),
		time: "000000".to_string(),
		of_month: 3,
		of_quarter: 3,
		of_year: 9,
	};
	let chained = chain_label_inputs(Some(&previous), now);
	assert_eq!(chained.of_month, 1);
	assert_eq!(chained.of_quarter, 1);
	assert_eq!(chained.of_year, 10);
}

#[test]
fn label_inputs_reset_everything_in_a_new_year() {
	let now = timestamp(2027, 1, 2);
	let previous = LabelInputs {
		date: "2026-12-31".to_string(),
		time: "000000".to_string(),
		of_month: 5,
		of_quarter: 3,
		of_year: 12,
	};
	let chained = chain_label_inputs(Some(&previous), now);
	assert_eq!(chained.of_month, 1);
	assert_eq!(chained.of_quarter, 1);
	assert_eq!(chained.of_year, 1);
}

#[test]
fn label_inputs_start_at_one_without_history() {
	let chained = chain_label_inputs(None, timestamp(2026, 9, 19));
	assert_eq!(chained.of_month, 1);
	assert_eq!(chained.of_quarter, 1);
	assert_eq!(chained.of_year, 1);
}

#[test]
fn label_inputs_treat_malformed_history_as_a_new_period() {
	let now = timestamp(2026, 9, 19);
	let previous = LabelInputs {
		date: "garbage".to_string(),
		time: "000000".to_string(),
		of_month: 9,
		of_quarter: 9,
		of_year: 9,
	};
	let chained = chain_label_inputs(Some(&previous), now);
	assert_eq!(chained.of_month, 1);
	assert_eq!(chained.of_quarter, 1);
	assert_eq!(chained.of_year, 1);
}

#[test]
fn label_inputs_detect_empty_default() {
	assert!(LabelInputs::default().is_empty());
	assert!(
		!LabelInputs {
			date: "2026-09-19".to_string(),
			..LabelInputs::default()
		}
		.is_empty()
	);
}

#[test]
fn hash_encodings_are_deterministic_and_distinct() {
	let digest = [0xde, 0xad, 0xbe, 0xef, 0x01, 0x23];
	let hex = encode_hash(&digest, HashEncoding::Hex, None);
	assert_eq!(hex, "deadbeef0123");
	assert_eq!(
		encode_hash(&digest, HashEncoding::Base32, None).len(),
		hex.len() * 4 / 5 + 1
	);
	assert!(
		encode_hash(&digest, HashEncoding::Base36, None)
			.chars()
			.all(|character| character.is_ascii_alphanumeric())
	);
	assert!(
		encode_hash(&digest, HashEncoding::Digits, None)
			.chars()
			.all(|character| character.is_ascii_digit())
	);
}

#[test]
fn hash_length_truncates_from_the_expected_end() {
	let digest = [0x00, 0x01, 0x02, 0x03, 0x04];
	let hex = encode_hash(&digest, HashEncoding::Hex, Some(4));
	assert_eq!(hex.len(), 4);
	assert!(hex.chars().all(|character| character.is_ascii_hexdigit()));

	let digits = encode_hash(&digest, HashEncoding::Digits, Some(3));
	assert_eq!(digits.len(), 3);
	assert!(digits.chars().all(|character| character.is_ascii_digit()));

	let zero_length = encode_hash(&digest, HashEncoding::Hex, Some(0));
	assert_eq!(zero_length, encode_hash(&digest, HashEncoding::Hex, None));
}

#[test]
fn hash_digits_of_zero_bytes_render_zero() {
	assert_eq!(encode_hash(&[0, 0, 0], HashEncoding::Digits, None), "0");
	assert_eq!(encode_hash(&[0, 0, 0], HashEncoding::Base36, None), "0");
	assert_eq!(encode_hash(&[], HashEncoding::Digits, None), "0");
}

#[test]
fn hash_digits_convert_big_endian_values() {
	// 0x0100 is 256.
	assert_eq!(encode_hash(&[1, 0], HashEncoding::Digits, None), "256");
}

#[test]
fn template_variables_include_context_and_values() {
	let inputs = LabelInputs {
		date: "2026-09-19".to_string(),
		time: "153045".to_string(),
		of_month: 2,
		of_quarter: 3,
		of_year: 4,
	};
	let mut values = BTreeMap::new();
	values.insert("build".to_string(), "17".to_string());
	let variables = template_variables("1.5.0", "", "app", "dart", &inputs, &values);
	assert_eq!(variables.get("identity"), Some(&"1.5.0".to_string()));
	assert_eq!(variables.get("year"), Some(&"2026".to_string()));
	assert_eq!(variables.get("year_short"), Some(&"26".to_string()));
	assert_eq!(variables.get("month"), Some(&"9".to_string()));
	assert_eq!(variables.get("month_padded"), Some(&"09".to_string()));
	assert_eq!(variables.get("quarter"), Some(&"3".to_string()));
	assert_eq!(variables.get("day"), Some(&"19".to_string()));
	assert_eq!(variables.get("date"), Some(&"20260919".to_string()));
	assert_eq!(variables.get("time"), Some(&"153045".to_string()));
	assert_eq!(variables.get("release_of_month"), Some(&"2".to_string()));
	assert_eq!(variables.get("release_of_quarter"), Some(&"3".to_string()));
	assert_eq!(variables.get("release_of_year"), Some(&"4".to_string()));
	assert_eq!(variables.get("build"), Some(&"17".to_string()));
}

#[test]
fn template_variables_tolerate_missing_calendar_context() {
	let mut values = BTreeMap::new();
	values.insert("build".to_string(), "1".to_string());
	let variables = template_variables("1.0.0", "", "app", "npm", &LabelInputs::default(), &values);
	assert!(!variables.contains_key("year"));
	assert_eq!(variables.get("build"), Some(&"1".to_string()));
}

#[test]
fn render_version_template_handles_both_spacing_forms() {
	let mut variables = BTreeMap::new();
	variables.insert("year".to_string(), "2026".to_string());
	variables.insert("month".to_string(), "9".to_string());
	assert_eq!(
		render_version_template("{{ year }}.{{ month }}", &variables),
		"2026.9"
	);
	assert_eq!(
		render_version_template("{{year}}.{{month}}", &variables),
		"2026.9"
	);
}

#[test]
fn render_version_template_prefers_the_longest_variable_name() {
	let mut variables = BTreeMap::new();
	variables.insert("build".to_string(), "1".to_string());
	variables.insert("build.android".to_string(), "17".to_string());
	assert_eq!(
		render_version_template("{{ build.android }}-{{ build }}", &variables),
		"17-1"
	);
}

#[test]
fn template_variables_used_lists_each_reference() {
	let used = template_variables_used("{{ year }}.{{ month }}-{{ build }}");
	assert_eq!(used, vec!["year", "month", "build"]);
	assert!(template_variables_used("no variables here").is_empty());
	assert!(template_variables_used("{{ unterminated").is_empty());
}

#[test]
fn validate_template_variables_accepts_declared_ids() {
	let mut available = BTreeMap::new();
	available.insert("year".to_string(), "2026".to_string());
	available.insert("build".to_string(), "1".to_string());
	validate_template_variables("{{ year }}.{{ build }}", &available, "test")
		.unwrap_or_else(|error| panic!("declared variables should validate: {error}"));
}

#[test]
fn validate_template_variables_rejects_unknown_names() {
	let mut available = BTreeMap::new();
	available.insert("year".to_string(), "2026".to_string());
	let error = validate_template_variables("{{ nope }}", &available, "release_title")
		.expect_err("unknown variable should fail");
	assert!(error.to_string().contains("unknown variable"));
}

#[test]
fn validate_template_variables_supports_build_axis_prefix() {
	let mut available = BTreeMap::new();
	available.insert("android".to_string(), "17".to_string());
	validate_template_variables("{{ build.android }}", &available, "test")
		.unwrap_or_else(|error| panic!("declared axis should validate: {error}"));

	let error = validate_template_variables("{{ build.ios }}", &available, "test")
		.expect_err("undeclared axis should fail");
	assert!(error.to_string().contains("unknown value"));
}

#[test]
fn validate_template_variables_reports_when_no_values_are_declared() {
	let error = validate_template_variables("{{ build.ios }}", &BTreeMap::new(), "test")
		.expect_err("missing axis should fail");
	assert!(error.to_string().contains("none declared"));
}

#[test]
fn validate_value_id_accepts_lowercase_identifiers() {
	validate_value_id("build").unwrap_or_else(|error| panic!("simple id: {error}"));
	validate_value_id("play_code").unwrap_or_else(|error| panic!("underscored id: {error}"));
	validate_value_id("build2").unwrap_or_else(|error| panic!("trailing digit: {error}"));
}

#[test]
fn validate_value_id_allows_build_as_an_id() {
	// `build` is the conventional counter id, so it must stay available even
	// though the `build.<axis>` form is special.
	validate_value_id("build").unwrap_or_else(|error| {
		panic!("build is a declared id, not a reserved context variable: {error}")
	});
}

#[test]
fn validate_value_id_rejects_reserved_and_malformed_ids() {
	assert!(validate_value_id("").is_err());
	assert!(validate_value_id("year").is_err());
	assert!(validate_value_id("identity").is_err());
	assert!(validate_value_id("label").is_err());
	assert!(validate_value_id("Build").is_err());
	assert!(validate_value_id("build-code").is_err());
	assert!(validate_value_id("2build").is_err());
}

#[test]
fn validate_value_definition_requires_exactly_one_source() {
	let none = ValueDefinition::default();
	assert!(validate_value_definition("v", &none).is_err());

	let both = value_definition("file = \"b.json\"\nfield = \"build\"\nhash = \"a.bin\"\n");
	assert!(validate_value_definition("v", &both).is_err());
}

#[test]
fn validate_value_definition_requires_file_and_field_together() {
	let file_only = value_definition("file = \"b.json\"\n");
	let error =
		validate_value_definition("build", &file_only).expect_err("file without field should fail");
	assert!(error.to_string().contains("without `field`"));

	let field_only = value_definition("field = \"build\"\n");
	let error = validate_value_definition("build", &field_only)
		.expect_err("field without file should fail");
	assert!(error.to_string().contains("without `file`"), "{error}");

	let empty_field = value_definition("file = \"b.json\"\nfield = \"\"\n");
	assert!(validate_value_definition("build", &empty_field).is_err());
}

#[test]
fn validate_value_definition_rejects_hash_options_without_hash() {
	let length = value_definition("env = \"BUILD\"\nlength = 4\n");
	assert!(validate_value_definition("v", &length).is_err());

	let encoding = value_definition("env = \"BUILD\"\nencoding = \"base36\"\n");
	assert!(validate_value_definition("v", &encoding).is_err());
}

#[test]
fn validate_value_definition_rejects_counter_options_on_derived_values() {
	let reset = value_definition("env = \"BUILD\"\nreset = \"version\"\n");
	assert!(validate_value_definition("v", &reset).is_err());

	let stamped = value_definition("env = \"BUILD\"\non_release = \"increment\"\n");
	assert!(validate_value_definition("v", &stamped).is_err());
}

#[test]
fn validate_value_definition_accepts_each_source() {
	validate_value_definition(
		"build",
		&value_definition("file = \"b.json\"\nfield = \"build\"\n"),
	)
	.unwrap_or_else(|error| panic!("file counter: {error}"));
	validate_value_definition("artifact", &value_definition("hash = \"a.bin\"\n"))
		.unwrap_or_else(|error| panic!("hash value: {error}"));
	validate_value_definition("run", &value_definition("env = \"RUN\"\n"))
		.unwrap_or_else(|error| panic!("env value: {error}"));
	validate_value_definition("rev", &value_definition("git = \"short_hash\"\n"))
		.unwrap_or_else(|error| panic!("git value: {error}"));
	validate_value_definition("when", &value_definition("timestamp = \"now\"\n"))
		.unwrap_or_else(|error| panic!("timestamp value: {error}"));
}

#[test]
fn value_definition_parses_defaults() {
	let definition = value_definition("file = \"build.json\"\nfield = \"build\"\n");
	assert_eq!(definition.on_release, None);
	assert_eq!(definition.stamp_behaviour(), StampBehaviour::Increment);
	assert_eq!(definition.reset, ResetPolicy::Never);
	assert!(definition.is_file_counter());
	assert!(definition.is_monotonic());
}

#[test]
fn value_definition_parses_derived_sources() {
	let hash = value_definition(
		"hash = \"app.aab\"\nalgorithm = \"sha256\"\nencoding = \"digits\"\nlength = 8\n",
	);
	assert_eq!(hash.encoding, HashEncoding::Digits);
	assert_eq!(hash.length, Some(8));
	assert!(!hash.is_monotonic());

	let git = value_definition("git = \"commit_count\"\n");
	assert_eq!(git.git, Some(GitSource::CommitCount));

	let timestamp = value_definition("timestamp = \"commit\"\n");
	assert_eq!(timestamp.timestamp, Some(TimestampSource::Commit));
}

#[test]
fn value_definition_rejects_unknown_keys() {
	let error = toml::from_str::<ValueDefinition>("file = \"a\"\nfield = \"b\"\nnope = 1\n")
		.expect_err("unknown key should fail");
	assert!(error.to_string().contains("nope"));
}

#[test]
fn value_definition_reports_declared_sources() {
	let definition = value_definition("git = \"short_hash\"\n");
	assert_eq!(definition.declared_sources(), vec!["git"]);

	let multiple = value_definition("env = \"A\"\ngit = \"short_hash\"\n");
	assert_eq!(multiple.declared_sources(), vec!["env", "git"]);
}

#[test]
fn value_is_monotonic_reflects_stamping() {
	let increment = value_definition("file = \"b.json\"\nfield = \"build\"\n");
	assert!(value_is_monotonic(&increment));

	let read_only =
		value_definition("file = \"b.json\"\nfield = \"build\"\non_release = \"none\"\n");
	assert!(!value_is_monotonic(&read_only));

	let derived = value_definition("hash = \"a.bin\"\n");
	assert!(!value_is_monotonic(&derived));
}

#[test]
fn counter_field_segments_split_on_dots() {
	assert_eq!(counter_field_segments("build"), vec!["build"]);
	assert_eq!(
		counter_field_segments("custom.data.build"),
		vec!["custom", "data", "build"]
	);
	assert!(counter_field_segments("").is_empty());
}

#[test]
fn counter_from_json_reads_nested_fields() {
	let document: serde_json::Value =
		serde_json::json!({ "build": 4, "custom": { "data": { "build": 17 } } });
	let path = Path::new("build.json");
	assert_eq!(
		counter_from_json(&document, path, "build").unwrap_or_else(|error| panic!("flat: {error}")),
		4
	);
	assert_eq!(
		counter_from_json(&document, path, "custom.data.build")
			.unwrap_or_else(|error| panic!("nested: {error}")),
		17
	);
}

#[test]
fn counter_from_json_reports_missing_fields() {
	let document: serde_json::Value = serde_json::json!({ "build": 1 });
	let error = counter_from_json(&document, Path::new("build.json"), "missing")
		.expect_err("missing field should fail");
	assert!(matches!(error, CounterFileError::MissingField { .. }));
	assert!(
		error
			.to_string()
			.contains("does not contain field `missing`")
	);
}

#[test]
fn counter_from_json_reports_non_integers() {
	let document: serde_json::Value = serde_json::json!({ "build": "four" });
	let error = counter_from_json(&document, Path::new("build.json"), "build")
		.expect_err("string counter should fail");
	assert!(matches!(error, CounterFileError::NotAnInteger { .. }));
	assert!(error.to_string().contains("non-negative integers"));
}

#[test]
fn counter_from_json_rejects_negative_and_fractional_values() {
	let negative: serde_json::Value = serde_json::json!({ "build": -1 });
	assert!(counter_from_json(&negative, Path::new("b.json"), "build").is_err());

	let fractional: serde_json::Value = serde_json::json!({ "build": 1.5 });
	assert!(counter_from_json(&fractional, Path::new("b.json"), "build").is_err());

	let boolean: serde_json::Value = serde_json::json!({ "build": true });
	assert!(counter_from_json(&boolean, Path::new("b.json"), "build").is_err());
}

#[test]
fn counter_file_errors_convert_into_config_errors() {
	let error = CounterFileError::MissingFile {
		path: PathBuf::from("build.json"),
	};
	let converted: crate::MonochangeError = error.into();
	assert!(converted.to_string().contains("does not exist"));
	assert!(
		converted
			.to_string()
			.contains("create it with its starting value")
	);
}

#[test]
fn renders_numeric_identifies_values_that_are_always_digits() {
	let counter = value_definition("file = \"b.json\"\nfield = \"build\"\n");
	assert!(counter.renders_numeric());

	let digits = value_definition("hash = \"a.bin\"\nencoding = \"digits\"\n");
	assert!(digits.renders_numeric());

	// base36 and friends can emit letters, so they are not numeric.
	let base36 = value_definition("hash = \"a.bin\"\nencoding = \"base36\"\n");
	assert!(!base36.renders_numeric());
	let hex = value_definition("hash = \"a.bin\"\nencoding = \"hex\"\n");
	assert!(!hex.renders_numeric());

	let env = value_definition("env = \"RUN\"\n");
	assert!(!env.renders_numeric());
	let timestamp = value_definition("timestamp = \"now\"\n");
	assert!(!timestamp.renders_numeric());
}

#[test]
fn renders_numeric_respects_read_only_counters() {
	// A read-only counter is still an integer, so it renders as digits.
	let read_only =
		value_definition("file = \"b.json\"\nfield = \"build\"\non_release = \"none\"\n");
	assert!(read_only.renders_numeric());
}

#[test]
fn template_variables_include_the_prerelease_and_name() {
	let mut values = BTreeMap::new();
	values.insert("build".to_string(), "9".to_string());
	let variables = template_variables(
		"1.2.3",
		"beta.1",
		"app",
		"cargo",
		&LabelInputs::default(),
		&values,
	);
	assert_eq!(variables.get("prerelease"), Some(&"beta.1".to_string()));
	assert_eq!(variables.get("name"), Some(&"app".to_string()));
	assert_eq!(variables.get("ecosystem"), Some(&"cargo".to_string()));
}

#[test]
fn render_version_template_leaves_unknown_variables_in_place() {
	let mut variables = BTreeMap::new();
	variables.insert("year".to_string(), "2026".to_string());
	assert_eq!(
		render_version_template("{{ year }}-{{ missing }}", &variables),
		"2026-{{ missing }}"
	);
}

#[test]
fn hash_length_beyond_the_digest_keeps_the_whole_value() {
	let digest = [0xab; 4];
	let hex = encode_hash(&digest, HashEncoding::Hex, Some(64));
	assert_eq!(hex, "abababab");
}

#[test]
fn base32_encoding_uses_the_rfc4648_alphabet() {
	let digest = [0xF8, 0x1F];
	let encoded = encode_hash(&digest, HashEncoding::Base32, None);
	assert!(
		encoded.chars().all(|character| {
			character.is_ascii_lowercase() || ('2'..='7').contains(&character)
		})
	);
}

#[test]
fn value_definition_stamp_behaviour_defaults_by_source() {
	// An explicit behaviour always wins.
	assert_eq!(
		value_definition("file = \"build.json\"\non_release = \"none\"\n").stamp_behaviour(),
		StampBehaviour::None
	);
	// A file counter with no explicit behaviour increments.
	assert_eq!(
		value_definition("file = \"build.json\"\n").stamp_behaviour(),
		StampBehaviour::Increment
	);
	// A non-file source with no explicit behaviour is never stamped.
	assert_eq!(
		value_definition("env = \"GITHUB_RUN_NUMBER\"\n").stamp_behaviour(),
		StampBehaviour::None
	);
}

#[test]
fn same_quarter_compares_the_calendar_quarter() {
	// Q3 2026 covers July through September.
	let august = timestamp(2026, 8, 15);
	assert!(august.same_quarter(&LabelInputs {
		date: "2026-07-01".to_string(),
		..LabelInputs::default()
	}));
	assert!(august.same_quarter(&LabelInputs {
		date: "2026-09-30".to_string(),
		..LabelInputs::default()
	}));
	// June is the previous quarter.
	assert!(!august.same_quarter(&LabelInputs {
		date: "2026-06-30".to_string(),
		..LabelInputs::default()
	}));
	// A different year never matches.
	assert!(!august.same_quarter(&LabelInputs {
		date: "2025-08-15".to_string(),
		..LabelInputs::default()
	}));
	// A malformed month cannot be compared, so it does not match.
	assert!(!august.same_quarter(&LabelInputs {
		date: "2026-xx-15".to_string(),
		..LabelInputs::default()
	}));
	// A date without a month has no quarter to compare.
	assert!(!august.same_quarter(&LabelInputs {
		date: "2026-".to_string(),
		..LabelInputs::default()
	}));
}

#[test]
fn render_version_template_skips_variables_without_a_value() {
	// A placeholder with no matching variable is left untouched rather than
	// being replaced with an empty string.
	let mut variables = BTreeMap::new();
	variables.insert("year".to_string(), "2026".to_string());
	assert_eq!(
		render_version_template("{{ year }}.{{ missing }}", &variables),
		"2026.{{ missing }}"
	);
}
