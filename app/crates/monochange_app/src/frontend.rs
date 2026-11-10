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
//! renders the same markup per frontend instead: a modulepreload for the
//! entry module, a wasm preload, leptos' hydration script with the
//! frontend's paths, and the [`WasmSplitManifest`] context the SSR stream
//! reads to preload the lazy chunks a page renders.

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

	/// This frontend's stylesheet URL.
	#[must_use]
	pub fn stylesheet(&self) -> String {
		format!("/{}/{}.css", self.pkg_url, self.output_name)
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

/// leptos 0.9's hydration script, loaded once per entry with that entry's
/// paths. Mirrors `leptos-0.9.0-rc1/src/hydration/hydration_script.js`
/// verbatim so the emitted document hydrates exactly as upstream would.
const HYDRATION_SCRIPT: &str = r#"(function (root, pkg_path, output_name, wasm_output_name, defer) {
	const hydrate = () =>
		import(`${root}/${pkg_path}/${output_name}.js`)
			.then(mod => {
				mod.default({module_or_path: `${root}/${pkg_path}/${wasm_output_name}.wasm`}).then(() => {
					mod.hydrate();
				});
			});
	if (defer && "requestIdleCallback" in window) {
		window.requestIdleCallback(hydrate, { timeout: 2000 });
	} else {
		hydrate();
	}
})
"#;

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

	if let Some((pkg_url, preloads, split_loader)) = manifest_for(&frontend, &options.site_root) {
		provide_context(WasmSplitManifest(
			leptos::reactive::owner::ArcStoredValue::new((pkg_url, preloads, split_loader)),
		));
	}

	// No defer: the website has always hydrated immediately, and the projects
	// entry keeps that behavior.
	let script = format!(
		"{HYDRATION_SCRIPT}({:?}, {:?}, {:?}, {:?}, {});",
		"",
		frontend.pkg_url,
		frontend.output_name,
		format!("{}_bg", frontend.output_name),
		false,
	);

	view! {
		<link rel="modulepreload" href=entry_js />
		<link rel="preload" href=entry_wasm r#as="fetch" r#type="application/wasm" />
		<script type="module">{script}</script>
	}
}

#[cfg(test)]
#[path = "__tests__/frontend_tests.rs"]
mod tests;
