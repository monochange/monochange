//! Theme tokens must stay aligned with docs/landing-design.md (--mc-*).

#[test]
fn accent_deep_matches_release_floor() {
	assert!(crate::theme::BUTTON_PRIMARY.contains("#0F766E"));
}

#[test]
fn page_ground_is_mist_not_purple() {
	assert!(crate::theme::PAGE.contains("#E8EEF4"));
	assert!(!crate::theme::PAGE.contains("#8b5cf6"));
	assert!(!crate::theme::PAGE.contains("violet"));
	assert!(!crate::theme::PAGE.contains("indigo"));
}
