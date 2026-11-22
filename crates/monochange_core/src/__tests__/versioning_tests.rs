#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;

use crate::versioning::GitSource;
use crate::versioning::HashAlgorithm;
use crate::versioning::HashEncoding;
use crate::versioning::ReleaseTimestamp;
use crate::versioning::ResetPolicy;
use crate::versioning::StampBehaviour;
use crate::versioning::TimestampSource;
use crate::versioning::ValueDefinition;
use crate::versioning::encode_hash;
use crate::versioning::validate_template_variables;
use crate::versioning::validate_value_definition;
use crate::versioning::validate_value_id;

fn value_definition(toml_source: &str) -> ValueDefinition {
	toml::from_str(toml_source)
		.unwrap_or_else(|error| panic!("value definition should parse: {error}"))
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
	assert_eq!(HashAlgorithm::Sha256.as_str(), "sha256");
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
fn release_timestamp_rejects_malformed_input() {
	assert!(ReleaseTimestamp::parse("2026-09", "153045").is_err());
	assert!(ReleaseTimestamp::parse("not-a-date", "153045").is_err());
	assert!(ReleaseTimestamp::parse("2026-09-19-01", "153045").is_err());
	assert!(ReleaseTimestamp::parse("2026-09-19", "1530").is_err());
	assert!(ReleaseTimestamp::parse("2026-09-19", "15z045").is_err());
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
