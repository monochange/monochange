//! Typed file-route recognition, hrefs, and the renderer boundary.

use leptoskit_router::Render;

use crate::routes::Route;
use crate::routes::SignIn;
use crate::routes::Workspace;
use crate::typed_routes::book;
use crate::typed_routes::href;
use crate::typed_routes::is_connect_return;
use crate::typed_routes::is_deferred_public_path;
use crate::typed_routes::parse;
use crate::typed_routes::recognized_path;

/// The website paths the Leptos route tree registers in `app.rs`.
const LEPTOS_WEBSITE_PATHS: &[&str] = &[
	"/",
	"/dashboard",
	"/install",
	"/connect",
	"/account",
	"/login",
	"/pricing",
	"/changelog",
	"/book",
	"/book/*chapter",
	"/auth/callback",
];

fn all_routes() -> Vec<Route> {
	vec![
		Route::Index,
		Route::Account,
		Route::BookChapter(crate::routes::BookChapter {
			chapter: vec!["guide".to_owned(), "first".to_owned()],
		}),
		Route::Changelog,
		Route::Connect(crate::routes::Connect {
			client: Some("native".to_owned()),
			redirect_uri: Some("dev.monochange.app:/auth/native/return".to_owned()),
			code_challenge: Some("challenge".to_owned()),
			state: Some("state-1234567890".to_owned()),
			code_challenge_method: Some("S256".to_owned()),
		}),
		Route::Workspace(Workspace {
			q: Some("web".to_owned()),
			org: Some("acme".to_owned()),
		}),
		Route::Install,
		Route::SignIn(SignIn {
			next: Some("/projects".to_owned()),
		}),
		Route::Pricing,
		Route::AuthCallback(crate::routes::AuthCallback {
			code: Some("code".to_owned()),
			state: Some("state".to_owned()),
		}),
	]
}

#[test]
fn every_generated_route_writes_an_href_that_parses_back() {
	for route in all_routes() {
		let written = href(&route);
		assert_eq!(parse(&written), Some(route), "{written}");
	}
}

#[test]
fn every_generated_route_converts_and_renders_its_boundary() {
	let navigation = crate::routes::Navigation::new(Route::Index);

	for route in all_routes() {
		let rendered = route.clone().render(&navigation);
		assert_eq!(rendered.text_content(), "", "{route:?}");
	}

	let typed = [
		Route::from(crate::routes::BookChapter {
			chapter: vec!["guide".to_owned()],
		}),
		Route::from(crate::routes::Connect {
			client: None,
			redirect_uri: None,
			code_challenge: None,
			state: None,
			code_challenge_method: None,
		}),
		Route::from(Workspace { q: None, org: None }),
		Route::from(SignIn { next: None }),
		Route::from(crate::routes::AuthCallback {
			code: None,
			state: None,
		}),
	];
	for route in typed {
		let rendered = route.clone().render(&navigation);
		assert_eq!(rendered.text_content(), "", "{route:?}");
	}
}

#[test]
fn the_generated_map_covers_the_website_paths() {
	let mut generated = vec![
		href(&Route::Index),
		href(&Route::Workspace(Workspace { q: None, org: None })),
		href(&Route::Install),
		href(&Route::Connect(crate::routes::Connect {
			client: None,
			redirect_uri: None,
			code_challenge: None,
			state: None,
			code_challenge_method: None,
		})),
		href(&Route::Account),
		href(&Route::SignIn(SignIn { next: None })),
		href(&Route::Pricing),
		href(&Route::Changelog),
		crate::typed_routes::book_root(),
		href(&Route::AuthCallback(crate::routes::AuthCallback {
			code: None,
			state: None,
		})),
	];
	generated.sort();
	generated.dedup();

	let mut leptos = LEPTOS_WEBSITE_PATHS
		.iter()
		.copied()
		.filter(|path| {
			// `/book` and `/book/*chapter` are the two pages of one generated
			// optional catch-all.
			*path != "/book"
		})
		.map(|path| {
			if path == "/book/*chapter" {
				"/book".to_owned()
			} else {
				path.to_owned()
			}
		})
		.collect::<Vec<_>>();
	leptos.sort();
	leptos.dedup();

	assert_eq!(generated, leptos);
}

#[test]
fn recognized_paths_require_the_canonical_spelling() {
	for &path in LEPTOS_WEBSITE_PATHS {
		let path = path.trim_end_matches("/*chapter");
		assert!(recognized_path(path).is_some(), "{path} must be recognized");
	}

	for alias in [
		"/%6cogin",
		"/login/",
		"/bookish",
		"/projects",
		"/p/org/project",
	] {
		assert!(
			recognized_path(alias).is_none(),
			"{alias} must stay outside the website route map"
		);
	}
}

#[test]
fn deferred_hydration_follows_the_generated_public_routes() {
	for path in ["/", "/install", "/pricing", "/changelog", "/login", "/book"] {
		assert!(is_deferred_public_path(path), "{path}");
	}
	assert!(is_deferred_public_path("/book/guide/first"));
	assert!(is_deferred_public_path("/book/"));

	for path in ["/dashboard", "/connect", "/account", "/auth/callback"] {
		assert!(!is_deferred_public_path(path), "{path}");
	}
	assert!(!is_deferred_public_path("/projects"));
}

#[test]
fn book_links_use_the_generated_catch_all() {
	assert_eq!(book(&[]), "/book");
	assert_eq!(
		book(&["guide".to_owned(), "website-release-notes".to_owned()]),
		"/book/guide/website-release-notes"
	);
	assert_eq!(book(&["a chapter".to_owned()]), "/book/a%20chapter");
}

#[test]
fn consent_returns_are_typed_connect_urls() {
	assert!(is_connect_return(
		"/connect?client=native&redirect_uri=dev.monochange.app%3A%2Fauth%2Fnative%2Freturn&code_challenge=challenge&state=state-1234567890"
	));
	assert!(is_connect_return(
		"/connect?client=native&state=request-123456789"
	));
	assert!(!is_connect_return("/connect"));
	assert!(!is_connect_return("/connect?x=bad\n"));
	assert!(!is_connect_return("/login?next=%2Fconnect"));
	assert!(!is_connect_return("//evil.example/connect?client=native"));
}

#[test]
fn generated_http_paths_expand_only_the_book_catch_all() {
	let paths = crate::typed_routes::http_paths();

	assert!(paths.contains(&"/book".to_owned()), "{paths:?}");
	assert!(paths.contains(&"/book/{*chapter}".to_owned()), "{paths:?}");
	assert!(!paths.contains(&"/{*chapter}".to_owned()), "{paths:?}");
	assert_eq!(
		paths.iter().filter(|path| path.contains("{*")).count(),
		1,
		"{paths:?}"
	);
	assert!(!paths.contains(&"/pkg".to_owned()), "{paths:?}");
}

#[test]
fn the_pinned_generator_catch_all_still_drops_its_prefix() {
	// `Pattern::axum_path` writes a trailing catch-all as `/{*name}`, which
	// would swallow `/pkg` and bypass the per-path middleware. Keep the
	// generated list out of the SSR router and register the Leptos tree
	// instead; this test makes the deliberate divergence explicit.
	let generated = crate::routes::axum_route_list();
	assert!(
		generated.contains(&"/{*chapter}".to_owned()),
		"{generated:?}"
	);
	assert!(!generated.contains(&"/book".to_owned()), "{generated:?}");
	assert!(!generated.contains(&"/book/{*chapter}".to_owned()));
}

#[test]
fn the_generated_renderer_still_returns_its_deferred_boundary() {
	let navigation = crate::routes::Navigation::new(Route::Index);
	let rendered = Route::Index.render(&navigation);
	assert_eq!(rendered.text_content(), "");
}
