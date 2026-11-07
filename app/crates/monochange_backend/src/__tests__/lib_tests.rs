#[test]
fn machine_api_router_symbol_matches_documented_signature() {
	let mount: fn(monochange_app_api::AppState) -> axum::Router<()> = crate::machine_api_router;
	let reexport: fn(monochange_app_api::AppState) -> axum::Router<()> = crate::api_router;
	assert_eq!(
		std::mem::size_of_val(&mount),
		std::mem::size_of_val(&reexport)
	);
}

#[test]
fn api_router_reexport_is_app_api_router() {
	let via_backend: fn(monochange_app_api::AppState) -> axum::Router<()> = crate::api_router;
	let via_api: fn(monochange_app_api::AppState) -> axum::Router<()> =
		monochange_app_api::api_router;
	assert_eq!(
		std::any::type_name_of_val(&via_backend),
		std::any::type_name_of_val(&via_api)
	);
}
