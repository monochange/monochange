//! Public documentation redirects and assets embedded with the book.

use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use axum::routing::get;

use crate::links::BOOK_URL;

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
#[cfg(test)]
#[path = "__tests__/public_routes_tests.rs"]
mod tests;
