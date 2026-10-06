//! Website-owned presentation of monochange's structured release notes.

use leptos::server;
use serde::Deserialize;
use serde::Serialize;

/// A published website version and its user-facing sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteRelease {
	pub version: String,
	pub summary: Vec<String>,
	pub sections: Vec<WebsiteSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteSection {
	pub title: String,
	pub entries: Vec<WebsiteEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteEntry {
	pub summary: String,
	/// Sanitized on the server before crossing the rendering boundary.
	pub details_html: String,
}

/// Load the release artifacts shipped with this application image.
#[server]
pub async fn get_website_releases() -> Result<Vec<WebsiteRelease>, server_fn::ServerFnError> {
	let site = std::env::var_os("LEPTOS_SITE_ROOT").map_or_else(
		|| std::path::PathBuf::from("target/site"),
		std::path::PathBuf::from,
	);
	load_releases(&site.join("releases"))
		.await
		.map_err(|error| {
			tracing::error!(%error, "website release notes could not be loaded");
			server_fn::ServerFnError::new("Release notes are unavailable right now.")
		})
}

/// Read only version-named JSON documents, newest stable version first.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn load_releases(
	directory: &std::path::Path,
) -> Result<Vec<WebsiteRelease>, Box<dyn std::error::Error + Send + Sync>> {
	let mut entries = match tokio::fs::read_dir(directory).await {
		Ok(entries) => entries,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
		Err(error) => return Err(error.into()),
	};
	let mut releases = Vec::new();
	while let Some(entry) = entries.next_entry().await? {
		let path = entry.path();
		if path.extension().and_then(|value| value.to_str()) != Some("json") {
			continue;
		}
		let version = semver::Version::parse(
			path.file_stem()
				.and_then(|value| value.to_str())
				.ok_or("invalid release filename")?,
		)?;
		let text = tokio::fs::read_to_string(&path).await?;
		let notes: monochange_core::ReleaseNotesDocument<monochange_core::ReleaseNotesEntry> =
			serde_json::from_str(&text)?;
		if notes.title != version.to_string() || !version.pre.is_empty() {
			return Err("release notes must match their stable version filename".into());
		}
		if notes
			.sections
			.iter()
			.flat_map(|section| &section.entries)
			.any(|entry| entry.stream != "website")
		{
			return Err("website release notes contain another audience stream".into());
		}
		releases.push((version, present_release(notes)));
	}
	releases.sort_by(|(left, _), (right, _)| right.cmp(left));
	Ok(releases.into_iter().map(|(_, release)| release).collect())
}

#[cfg(not(target_arch = "wasm32"))]
fn present_release(
	notes: monochange_core::ReleaseNotesDocument<monochange_core::ReleaseNotesEntry>,
) -> WebsiteRelease {
	WebsiteRelease {
		version: notes.title,
		summary: notes.summary,
		sections: notes
			.sections
			.into_iter()
			.map(|section| {
				WebsiteSection {
					title: section.title,
					entries: section
						.entries
						.into_iter()
						.map(|entry| {
							let mut html = String::new();
							pulldown_cmark::html::push_html(
								&mut html,
								pulldown_cmark::Parser::new(
									entry.details_markdown.as_deref().unwrap_or_default(),
								),
							);
							WebsiteEntry {
								summary: entry.summary,
								details_html: ammonia::clean(&html),
							}
						})
						.collect(),
				}
			})
			.collect(),
	}
}

#[cfg(test)]
#[path = "__tests__/releases_tests.rs"]
mod tests;
