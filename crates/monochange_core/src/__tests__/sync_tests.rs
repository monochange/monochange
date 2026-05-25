//! Tests for VersionStrategy and DependencySyncChange types.

use crate::DependencySyncChange;
use crate::VersionStrategy;

#[test]
fn version_strategy_variants_exist() {
	// Verify all variants can be constructed.
	let _default = VersionStrategy::Default;
	let _exact = VersionStrategy::Exact;
	let _caret = VersionStrategy::Caret;
	let _compatible = VersionStrategy::Compatible;
}

#[test]
fn version_strategy_default_is_default() {
	assert!(matches!(VersionStrategy::Default, VersionStrategy::Default));
}

#[test]
fn version_strategy_equality() {
	assert_eq!(VersionStrategy::Default, VersionStrategy::Default);
	assert_ne!(VersionStrategy::Default, VersionStrategy::Exact);
}

#[test]
fn dependency_sync_change_fields() {
	let change = DependencySyncChange {
		dependency_name: "my_package".to_string(),
		section: "dependencies".to_string(),
		old_value: "^0.5.0".to_string(),
		new_value: "^0.7.0".to_string(),
	};
	assert_eq!(change.dependency_name, "my_package");
	assert_eq!(change.section, "dependencies");
	assert_eq!(change.old_value, "^0.5.0");
	assert_eq!(change.new_value, "^0.7.0");
}
