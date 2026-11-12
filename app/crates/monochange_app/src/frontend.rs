//! The two browser frontends and their server-side hydration contracts.
//!
//! One backend serves both browser entries. The leptos router intercepts
//! same-origin anchor clicks, so the composition dispatches each request to
//! the frontend that owns its path and renders that frontend's hydration
//! scripts, stylesheet, and route tree.
//!
//! leptos 0.9's `HydrationScripts` resolves entry file names and the split
//! manifest through process-global `OnceLock`s keyed to the first caller, so
//! one process cannot serve two output names through it. [`FrontendScripts`]
//! selects preloads and hydration timing per document. Editable pages keep
//! eager entry and lazy-route preloads; public pages wait for interaction
//! or idle and replay button clicks after their routes attach.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use leptos::WasmSplitManifest;
use leptos::prelude::*;
use leptos_router::location::RequestUrl;
use monochange_shared::routing::SPLIT_LOADER;
use monochange_shared::routing::SPLIT_MANIFEST;
use tracing::warn;

/// One browser entry's bundle coordinates.
///
/// The pkg namespace and output name must match what the frontend's
/// `leptoskit.toml` build produces: the plugin places the client bundle in
/// `pkg/<namespace>/` and names it after the client lib crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frontend {
	/// The pkg directory, relative to the site root, holding this bundle.
	pub pkg_url: &'static str,
	/// The browser entry's file stem: the client lib crate's name.
	pub output_name: &'static str,
}

impl Frontend {
	/// The projects entry (product routes and feedback portals).
	pub const PROJECTS: Self = Self {
		pkg_url: monochange_shared::routing::PROJECTS_PKG_URL,
		output_name: monochange_shared::routing::PROJECTS_OUTPUT_NAME,
	};
	/// The website entry (landing, auth, docs, repositories dashboard).
	pub const WEBSITE: Self = Self {
		pkg_url: monochange_shared::routing::WEBSITE_PKG_URL,
		output_name: monochange_shared::routing::WEBSITE_OUTPUT_NAME,
	};

	/// The URL of this entry's hydration module.
	#[must_use]
	pub fn entry_js(&self) -> String {
		format!("/{}/{}.js", self.pkg_url, self.output_name)
	}

	/// The URL of this entry's wasm module.
	#[must_use]
	pub fn entry_wasm(&self) -> String {
		format!("/{}/{}_bg.wasm", self.pkg_url, self.output_name)
	}

	/// The stylesheet shared by both frontends.
	#[must_use]
	pub fn stylesheet(&self) -> String {
		monochange_shared::routing::WEBSITE_STYLESHEET.to_owned()
	}

	/// The site-root path of this frontend's split manifest.
	#[must_use]
	pub fn manifest_path(&self, site_root: &str) -> PathBuf {
		PathBuf::from(site_root)
			.join(self.pkg_url)
			.join(SPLIT_MANIFEST)
	}

	/// The frontend that owns the SSR request path.
	///
	/// Must run inside the leptos render owner: the request path comes from
	/// the `RequestUrl` context `leptos_axum` provides for every response,
	/// including the fallback error shell.
	#[must_use]
	pub fn current() -> Self {
		let path = use_context::<RequestUrl>()
			.and_then(|request| request.parse().ok())
			.map_or_else(String::new, |url| url.path().to_owned());

		if monochange_shared::routing::is_projects_path(&path) {
			Self::PROJECTS
		} else {
			Self::WEBSITE
		}
	}
}

/// The small loader waits for input on public documents and replays WASM controls.
const HYDRATION_SCRIPT: &str = include_str!("hydration.js");

/// Only these documents have links and buttons, with no editable form state.
fn defer_hydration(path: &str, signed_out: bool) -> bool {
	matches!(
		path,
		"/" | "/install" | "/pricing" | "/changelog" | "/login" | "/book"
	) || path.starts_with("/book/")
		|| (path == "/projects" && signed_out)
}

/// Parsed split manifests per frontend: `(pkg url prefix, preloads, loader)`.
type SplitManifest = (String, HashMap<String, Vec<String>>, String);

static MANIFESTS: Mutex<Option<HashMap<String, Option<SplitManifest>>>> = Mutex::new(None);

fn manifest_for(frontend: &Frontend, site_root: &str) -> Option<SplitManifest> {
	// Keyed by site root as well: the running server has one stable site
	// root, and tests render several fixtures against one process.
	let cache_key = format!("{site_root}\u{0}{}", frontend.pkg_url);
	// A poisoned cache lock still holds valid cached entries.
	let mut cache = MANIFESTS
		.lock()
		.unwrap_or_else(std::sync::PoisonError::into_inner);
	let cached = cache
		.get_or_insert_with(HashMap::new)
		.entry(cache_key)
		.or_insert_with(|| read_manifest(frontend, site_root));
	cached.clone()
}

fn read_manifest(frontend: &Frontend, site_root: &str) -> Option<SplitManifest> {
	let path = frontend.manifest_path(site_root);

	let raw = match std::fs::read_to_string(&path) {
		Ok(raw) => raw,
		Err(error) => {
			warn!("no split manifest at {}: {error}", path.display());
			return None;
		}
	};

	match serde_json::from_str::<HashMap<String, Vec<String>>>(&raw) {
		Ok(preloads) => {
			Some((
				format!("/{}", frontend.pkg_url),
				preloads,
				SPLIT_LOADER.to_owned(),
			))
		}
		Err(error) => {
			warn!("invalid split manifest at {}: {error}", path.display());
			None
		}
	}
}

/// The hydration head for one frontend: entry preloads and the split-manifest
/// context the SSR stream reads for lazy-chunk preloads.
///
/// Renders nothing on the client: hydration mounts the route tree over the
/// server document, and these tags belong to the server-rendered head only.
#[component]
#[allow(clippy::needless_pass_by_value)] // leptos components take props by value
pub fn FrontendScripts(
	/// The leptos options whose `site_root` holds the built site.
	options: LeptosOptions,
	/// The frontend whose bundle this document loads.
	frontend: Frontend,
) -> impl IntoView {
	let entry_js = frontend.entry_js();
	let entry_wasm = frontend.entry_wasm();
	let path = use_context::<RequestUrl>()
		.and_then(|request| request.parse().ok())
		.map_or_else(String::new, |url| url.path().to_owned());
	let signed_out = use_context::<axum::http::request::Parts>()
		.and_then(|parts| {
			parts
				.extensions
				.get::<monochange_app_api::sessions::Principal>()
				.cloned()
		})
		.is_some_and(|principal| {
			matches!(
				principal,
				monochange_app_api::sessions::Principal::Anonymous
					| monochange_app_api::sessions::Principal::Stale
			)
		});
	let defer = defer_hydration(&path, signed_out);

	if let Some((pkg_url, preloads, split_loader)) = (!defer)
		.then(|| manifest_for(&frontend, &options.site_root))
		.flatten()
	{
		provide_context(WasmSplitManifest(
			leptos::reactive::owner::ArcStoredValue::new((pkg_url, preloads, split_loader)),
		));
	}

	// The server selects the policy; the loader never guesses from browser URLs.
	let loader = HYDRATION_SCRIPT.trim().trim_end_matches(';');
	let script = format!(
		"{loader}({:?}, {:?}, {:?}, {:?}, {});",
		"",
		frontend.pkg_url,
		frontend.output_name,
		format!("{}_bg", frontend.output_name),
		defer,
	);

	view! {
		<link rel="preload" href="/fonts/bricolage-800.woff2" r#as="font" r#type="font/woff2" crossorigin="anonymous" />
		<link rel="preload" href="/fonts/hanken-400.woff2" r#as="font" r#type="font/woff2" crossorigin="anonymous" />
		<link rel="preload" href="/fonts/hanken-700.woff2" r#as="font" r#type="font/woff2" crossorigin="anonymous" />
		{(!defer).then(|| view! {
			<link rel="modulepreload" href=entry_js />
			<link rel="preload" href=entry_wasm r#as="fetch" r#type="application/wasm" crossorigin="anonymous" />
		})}
		<script r#type=(!defer).then_some("module") data-deferred-hydration=defer.to_string()>{script}</script>
	}
}

#[cfg(test)]
#[path = "__tests__/frontend_tests.rs"]
mod tests;
