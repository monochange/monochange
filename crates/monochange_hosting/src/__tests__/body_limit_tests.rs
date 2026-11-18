use super::bound_body;

#[test]
fn bodies_reserve_notice_and_keep_complete_unicode_entries() {
	assert_eq!(
		bound_body("éé\nnext", &[0, 5, 9, usize::MAX, 1], "...", 6),
		"éé\n..."
	);
	assert_eq!(bound_body("abc", &[], "long notice", 3), "abc");
	assert_eq!(bound_body("abcdef", &[], "ééé", 2), "éé");
	assert_eq!(bound_body("abcdef", &[], "...", 4), "...");
	assert_eq!(bound_body("abcdef", &[3], "...", 5), "...");
	assert_eq!(bound_body("abcdef", &[3], "...", 0), "");
}
