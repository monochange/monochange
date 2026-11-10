use leptos::prelude::*;
use leptos_router::location::RequestUrl;

#[test]
fn a_query_stays_on_its_route_and_home_is_registered() {
	assert!(matches!(super::home(), crate::routes::Route::Index));

	Owner::new().with(|| {
		provide_context(RequestUrl::new("http://monochange.dev/pricing"));
		assert!(matches!(
			super::route_from_request(),
			crate::routes::Route::Pricing
		));
	});
	Owner::new().with(|| {
		provide_context(RequestUrl::new("http://monochange.dev/pricing?plan=free"));
		assert!(matches!(
			super::route_from_request(),
			crate::routes::Route::Pricing
		));
	});
}
