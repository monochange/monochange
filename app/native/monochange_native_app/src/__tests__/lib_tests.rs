//! Theme utilities must track Release floor (teal brand, not purple).

#[test]
fn primary_button_uses_teal_utilities() {
	assert!(crate::theme::BUTTON_PRIMARY.contains("bg-teal-700"));
	assert!(!crate::theme::BUTTON_PRIMARY.contains("violet"));
	assert!(!crate::theme::BUTTON_PRIMARY.contains("indigo"));
	assert!(!crate::theme::BUTTON_PRIMARY.contains("#8b5cf6"));
}

#[test]
fn page_uses_slate_mist_not_purple() {
	assert!(crate::theme::PAGE.contains("bg-slate-100"));
	assert!(!crate::theme::PAGE.contains("violet"));
	assert!(!crate::theme::PAGE.contains("#f5f3ff"));
}
