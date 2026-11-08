//! Public documentation redirects and assets embedded with the book.

use std::path::PathBuf;

use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use axum::routing::get;
use leptos::prelude::LeptosOptions;
use tower_http::services::ServeFile;

use crate::links::BOOK_URL;

/// Route + file service that maps cargo-leptos's on-disk `pkg/<name>.wasm` to the
/// `pkg/<name>_bg.wasm` URL HydrationScripts emits when `LEPTOS_OUTPUT_NAME` was
/// unset while compiling `leptos` (shared CI/dep caches often hit this).
///
/// Without this alias the browser 404s the hydrate wasm even though
/// `monochange_app.wasm` is present; the ignored Chrome fixture has needed the
/// same remap since #761.
pub fn pkg_wasm_bg_alias(options: &LeptosOptions) -> (String, ServeFile) {
	let mut route = options.site_pkg_dir_route_base();
	route.push_str(&options.output_name);
	route.push_str("_bg.wasm");
	let file = PathBuf::from(options.site_root.as_ref())
		.join(options.site_pkg_dir.as_ref())
		.join(format!("{}.wasm", options.output_name));
	(route, ServeFile::new(file))
}

/// Documentation destinations served without authentication or application state.
pub fn book_redirects() -> Router {
	Router::new()
		.route("/docs", get(|| async { Redirect::permanent(BOOK_URL) }))
		.route("/docs/", get(|| async { Redirect::permanent(BOOK_URL) }))
		.route(
			"/docs/{*chapter}",
			get(|Path(chapter): Path<String>| {
				async move { Redirect::permanent(&crate::book::chapter_href(&chapter)) }
			}),
		)
		.route(
			"/book-assets/{*asset}",
			get(|Path(asset): Path<String>| async move { book_asset(&asset) }),
		)
}

fn book_asset(asset: &str) -> Response {
	let mime = match std::path::Path::new(asset)
		.extension()
		.and_then(|extension| extension.to_str())
	{
		Some("svg") => "image/svg+xml",
		Some("png") => "image/png",
		Some("json") => "application/json",

		_ => return StatusCode::NOT_FOUND.into_response(),
	};

	match crate::book::BOOK_FILES.get_file(asset) {
		Some(file) => {
			(
				[
					(header::CONTENT_TYPE, mime),
					(header::CACHE_CONTROL, "public, max-age=3600"),
				],
				file.contents(),
			)
				.into_response()
		}
		None => StatusCode::NOT_FOUND.into_response(),
	}
}

/// Serves `/pkg` hydration assets with revalidation caching: the static file
/// handler answers `If-Modified-Since` with cheap 304s, so browsers reuse the
/// cached WASM chunks while a deployment can never pair fresh HTML with stale
/// client code. The image ships precompressed `.gz`/`.br` siblings that the
/// file handler serves to clients that accept them.
pub async fn pkg_cache_control(
	request: axum::extract::Request,
	next: axum::middleware::Next,
) -> Response {
	let is_pkg_asset = request.uri().path().starts_with("/pkg/");
	let mut response = next.run(request).await;
	if is_pkg_asset {
		response.headers_mut().insert(
			header::CACHE_CONTROL,
			axum::http::HeaderValue::from_static("public, max-age=0, must-revalidate"),
		);
	}
	response
}
#[cfg(test)]
#[path = "__tests__/public_routes_tests.rs"]
mod tests;
