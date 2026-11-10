use leptoskit_router::Routable;

use crate::routes::Route;

#[test]
fn the_floor_and_install_are_public_routes() {
	assert!(matches!(Route::parse("/").unwrap(), Route::Index));
	assert!(matches!(Route::parse("/install").unwrap(), Route::Install));
	assert_eq!(Route::Index.href().as_str(), "/");
	assert_eq!(Route::Install.href().as_str(), "/install");
}

#[test]
fn the_stack_returns_to_the_floor() {
	let navigation = crate::routes::Navigation::new(Route::Index);
	assert!(!navigation.can_go_back());
	navigation.navigate(Route::Install);
	assert!(matches!(navigation.current(), Route::Install));
	assert!(navigation.can_go_back());
	assert!(navigation.back());
	assert!(matches!(navigation.current(), Route::Index));
	assert!(!navigation.back());
}
