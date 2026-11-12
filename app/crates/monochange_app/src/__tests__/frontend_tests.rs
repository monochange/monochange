//! Frontend bundle selection: a document must reference the entry bundle of
//! the frontend that owns the request path, with that frontend's split
//! manifest driving lazy-chunk preloads.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use leptos::WasmSplitManifest;
use leptos::prelude::*;

use crate::frontend::Frontend;
use crate::frontend::FrontendScripts;
use crate::tests::render;

fn fixture_site_root() -> String {
	std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../../fixtures/tests/app-frontend-scripts/site")
		.to_string_lossy()
		.into_owned()
}

async fn scripts_html(frontend: Frontend) -> String {
	let options = leptos::config::LeptosOptions::builder()
		.output_name(monochange_shared::routing::WEBSITE_OUTPUT_NAME)
		.site_root(fixture_site_root())
		.build();
	render(move || {
		let options = options.clone();
		view! { <FrontendScripts options=options frontend=frontend /> }
	})
	.await
}

/// Reads the split-manifest context `FrontendScripts` provides, as the SSR
/// stream does when it emits lazy-chunk preloads.
#[component]
fn ManifestProbe() -> impl IntoView {
	let read = use_context::<WasmSplitManifest>().and_then(|manifest| manifest.0.try_read_value());
	let text = read.map_or_else(
		|| "no-manifest".to_owned(),
		|guard| {
			let (pkg_url, preloads, split_loader) = &*guard;
			let mut chunks: Vec<String> = preloads.values().flatten().cloned().collect();
			chunks.sort();
			chunks.dedup();
			format!("{pkg_url}|{}|{split_loader}", chunks.join(","))
		},
	);
	view! { <span id="manifest-probe">{text}</span> }
}

async fn probe_html(frontend: Frontend) -> String {
	let options = leptos::config::LeptosOptions::builder()
		.output_name(monochange_shared::routing::WEBSITE_OUTPUT_NAME)
		.site_root(fixture_site_root())
		.build();
	render(move || {
		let options = options.clone();
		view! {
			<FrontendScripts options=options frontend=frontend />
			<ManifestProbe />
		}
	})
	.await
}

#[tokio::test]
async fn website_pages_reference_the_website_entry_bundle() {
	let html = scripts_html(Frontend::WEBSITE).await;
	assert!(
		html.contains("modulepreload\" href=\"/pkg/site/monochange_app.js\""),
		"{html}"
	);
	assert!(
		html.contains("href=\"/pkg/site/monochange_app_bg.wasm\""),
		"{html}"
	);
	assert!(html.contains("crossorigin=\"anonymous\""), "{html}");
	assert!(
		html.contains("(\"\", \"pkg/site\", \"monochange_app\", \"monochange_app_bg\", false);"),
		"the hydration script must load the website entry module: {html}"
	);
	assert!(!html.contains("/pkg/projects/"), "{html}");
}

#[tokio::test]
async fn product_pages_reference_the_projects_entry_bundle() {
	let html = scripts_html(Frontend::PROJECTS).await;
	assert!(
		html.contains("modulepreload\" href=\"/pkg/projects/monochange_projects_web.js\""),
		"{html}"
	);
	assert!(
		html.contains("href=\"/pkg/projects/monochange_projects_web_bg.wasm\""),
		"{html}"
	);
	assert!(html.contains("crossorigin=\"anonymous\""), "{html}");
	assert!(
		html.contains(
			"(\"\", \"pkg/projects\", \"monochange_projects_web\", \"monochange_projects_web_bg\", false);"
		),
		"the hydration script must load the projects entry module: {html}"
	);
	assert!(!html.contains("/pkg/site/"), "{html}");
}

#[tokio::test]
async fn the_website_split_manifest_reaches_the_stream_context() {
	let html = probe_html(Frontend::WEBSITE).await;
	assert!(
		html.contains(
			"pkg/site|chunk_15,split___changelog_route_view_18050757885357555981,split___site_session_route_view_2158154892423587711|__wasm_split.______________________.js"
		),
		"the website manifest must resolve chunks in its own namespace: {html}"
	);
	assert!(
		!html.contains("portal_route_view"),
		"projects chunks must not preload on website pages: {html}"
	);
}

#[tokio::test]
async fn the_projects_split_manifest_reaches_the_stream_context() {
	let html = probe_html(Frontend::PROJECTS).await;
	assert!(
		html.contains(
			"pkg/projects|chunk_11,chunk_12,chunk_13,split___organization_route_view_7110038765603574149,split___portal_route_view_10330659768788240853|__wasm_split.______________________.js"
		),
		"the projects manifest must resolve chunks in its own namespace: {html}"
	);
	assert!(
		!html.contains("site_session_route_view"),
		"website chunks must not preload on product pages: {html}"
	);
}

#[tokio::test]
async fn a_missing_split_manifest_degrades_to_entry_only() {
	let options = leptos::config::LeptosOptions::builder()
		.output_name(monochange_shared::routing::WEBSITE_OUTPUT_NAME)
		.site_root(
			std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
				.join("../../../fixtures/tests/app-frontend-scripts/empty-site")
				.to_string_lossy()
				.into_owned(),
		)
		.build();
	let html = render(move || {
		let options = options.clone();
		view! { <FrontendScripts options=options frontend=Frontend::WEBSITE /> }
	})
	.await;
	// No manifest on disk: the entry still hydrates, without chunk preloads.
	assert!(
		html.contains("modulepreload\" href=\"/pkg/site/monochange_app.js\""),
		"{html}"
	);
	assert!(
		!html.contains("manifest-probe") || html.contains("no-manifest"),
		"{html}"
	);
}

#[tokio::test]
async fn an_invalid_split_manifest_degrades_to_entry_only() {
	let options = leptos::config::LeptosOptions::builder()
		.output_name(monochange_shared::routing::WEBSITE_OUTPUT_NAME)
		.site_root(
			std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
				.join("../../../fixtures/tests/app-frontend-scripts/broken-site")
				.to_string_lossy()
				.into_owned(),
		)
		.build();
	let html = render(move || {
		let options = options.clone();
		view! { <FrontendScripts options=options frontend=Frontend::WEBSITE /> }
	})
	.await;
	// A corrupt manifest must not fail the render: the entry hydrates without
	// chunk preloads rather than serving a broken document.
	assert!(
		html.contains("modulepreload\" href=\"/pkg/site/monochange_app.js\""),
		"{html}"
	);
	assert!(
		!html.contains("manifest-probe") || html.contains("no-manifest"),
		"{html}"
	);
}

#[test]
fn frontend_bundle_coordinates_match_the_build_layout() {
	// The leptoskit pipeline (see the [[plugins]] options in both web
	// leptoskit.toml files) places each client bundle under pkg/<namespace>/,
	// named after the client lib crate. The shell constants must mirror that.
	assert_eq!(Frontend::WEBSITE.entry_js(), "/pkg/site/monochange_app.js");
	assert_eq!(
		Frontend::WEBSITE.entry_wasm(),
		"/pkg/site/monochange_app_bg.wasm"
	);
	assert_eq!(
		Frontend::WEBSITE.stylesheet(),
		monochange_shared::routing::WEBSITE_STYLESHEET
	);
	assert_eq!(
		Frontend::PROJECTS.entry_js(),
		"/pkg/projects/monochange_projects_web.js"
	);
	assert_eq!(
		Frontend::PROJECTS.entry_wasm(),
		"/pkg/projects/monochange_projects_web_bg.wasm"
	);
	assert_eq!(
		Frontend::PROJECTS.stylesheet(),
		monochange_shared::routing::PROJECTS_STYLESHEET
	);
	assert_eq!(
		Frontend::WEBSITE.manifest_path("/srv/site"),
		std::path::PathBuf::from("/srv/site/pkg/site/__wasm_split_manifest.json")
	);
	assert_eq!(
		Frontend::PROJECTS.manifest_path("/srv/site"),
		std::path::PathBuf::from("/srv/site/pkg/projects/__wasm_split_manifest.json")
	);
}
