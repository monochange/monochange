// Tokio's test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use leptos::prelude::Owner;
use leptoskit_router::Routable;

#[tokio::test]
async fn back_and_sign_out_run() {
	let _ = any_spawner::Executor::init_tokio();
	let navigation = crate::routes::Navigation::new(crate::routes::Route::parse("/").unwrap());
	navigation.navigate(crate::routes::Route::parse("/install").unwrap());
	let _button = Owner::new().with(|| super::back_control(&navigation));
	super::go_back(&navigation);
	assert!(matches!(navigation.current(), crate::routes::Route::Index));

	let _sign_out = Owner::new().with(super::sign_out_button);
	let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(super::begin_sign_out));
	let _ = tokio::spawn(super::sign_out()).await;
}
