use super::*;

#[test]
fn detailed_rule_config_merges_options() {
	let left = LintRuleConfig::Detailed {
		level: LintSeverity::Warning,
		options: BTreeMap::from([("fix".to_string(), serde_json::Value::Bool(false))]),
	};
	let right = LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("fields".to_string(), serde_json::json!(["description"]))]),
	};

	let merged = left.merged_with(&right);
	assert_eq!(merged.severity(), LintSeverity::Error);
	assert!(!merged.bool_option("fix", true));
	assert_eq!(
		merged.string_list_option("fields"),
		Some(vec!["description".to_string()])
	);
}

#[test]
fn lint_target_and_context_expose_parsed_payloads() {
	let target = LintTarget::new(
		".",
		"Cargo.toml",
		"contents",
		LintTargetMetadata {
			ecosystem: "cargo".to_string(),
			relative_path: PathBuf::from("Cargo.toml"),
			package_name: Some("core".to_string()),
			package_id: Some("core".to_string()),
			group_id: None,
			managed: true,
			private: Some(false),
			publishable: Some(true),
		},
		Box::new(42usize),
	);
	let ctx = LintContext {
		workspace_root: target.workspace_root.as_path(),
		manifest_path: target.manifest_path.as_path(),
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	assert_eq!(ctx.parsed_as::<usize>(), Some(&42));
	assert!(format!("{target:?}").contains("contents_len"));
	assert!(format!("{ctx:?}").contains("metadata"));
}
