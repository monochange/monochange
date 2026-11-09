use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestTarget;
use monochange_core::SourceConfiguration;
use monochange_hosting::body_limit::bound_body;

pub(super) fn bounded_release_body(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
	target: &ReleaseManifestTarget,
	body: &str,
) -> String {
	let limit = source
		.releases
		.max_body_chars
		.unwrap_or(125_000)
		.min(125_000);
	let path = manifest
		.changelogs
		.iter()
		.find(|changelog| {
			changelog.owner_id == target.id
				&& changelog.owner_kind == target.kind
				&& changelog.output == source.releases.changelog_output
		})
		.map_or_else(
			|| "changelog.md".to_owned(),
			|changelog| changelog.path.to_string_lossy().into_owned(),
		);
	let host = source.host.as_deref().unwrap_or("https://github.com");
	let path = path
		.split('/')
		.map(|part| urlencoding::encode(part).into_owned())
		.collect::<Vec<_>>()
		.join("/");
	let url = format!(
		"{}/{}/{}/blob/{}/{}",
		host.trim_end_matches('/'),
		source.owner,
		source.repo,
		urlencoding::encode(&target.tag_name),
		path
	);
	let notice = format!("\n\nFor more, [view the changelog for the full details]({url}).");
	bound_body(body, &entry_boundaries(body), &notice, limit)
}

// Only split at top-level note/section boundaries. Headings in fenced code
// examples are content, never safe truncation points. Non-Markdown formats
// fall back to the changelog pointer rather than emitting a partial document.
fn entry_boundaries(body: &str) -> Vec<usize> {
	let mut boundaries = Vec::new();
	let mut offset = 0;
	let mut section_start = None;
	let mut fence: Option<(char, usize)> = None;
	for line in body.split_inclusive('\n') {
		let trimmed = line.trim_start();
		let marker = trimmed.chars().next().unwrap_or_default();
		let count = trimmed.chars().take_while(|&c| c == marker).count();
		if matches!(marker, '`' | '~') && count >= 3 {
			match fence {
				None => fence = Some((marker, count)),
				Some((open, width))
					if marker == open
						&& count >= width && trimmed.trim_end().chars().all(|c| c == marker) =>
				{
					fence = None;
				}
				_ => {}
			}
		} else if fence.is_none() && line.starts_with("## ") {
			boundaries.push(offset);
			section_start = Some(offset);
		} else if fence.is_none() && line.starts_with("### ") {
			boundaries.push(section_start.take().unwrap_or(offset));
		}
		offset += line.len();
	}
	boundaries
}

#[cfg(test)]
#[path = "__tests__/body_tests.rs"]
mod tests;
