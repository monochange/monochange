//! Shared frozen release cut branch naming, metadata markers, and status banner rendering.

use std::fmt::Write as _;

use monochange_core::ReleaseManifest;
use monochange_core::SourceConfiguration;
use monochange_core::VersionFormat;
use serde::Deserialize;
use serde::Serialize;

/// Opening HTML comment marker for the embedded release cut metadata JSON block.
pub const CUT_META_START: &str = "<!-- monochange:cut-meta:start";
/// Closing HTML comment marker for the embedded release cut metadata JSON block.
pub const CUT_META_END: &str = "monochange:cut-meta:end -->";
/// Opening HTML comment marker for the in-body sticky cut status banner.
pub const CUT_STATUS_START: &str = "<!-- monochange:cut-status:start -->";
/// Closing HTML comment marker for the in-body sticky cut status banner.
pub const CUT_STATUS_END: &str = "<!-- monochange:cut-status:end -->";

/// Resolved branch pair and identifier for one frozen release cut.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResolvedCutBranches {
	/// Identifier for this cut (for example `v0.22.0` or `batch-1a2b3c4d`).
	pub cut_id: String,
	/// Source branch the cut was taken from (for example `main`).
	pub source_base_branch: String,
	/// Frozen base branch created from `source_base_branch` (for example `release/v0.22.0`).
	pub cut_base_branch: String,
	/// Head branch carrying the release commit (for example `monochange/release/v0.22.0`).
	pub head_branch: String,
}

/// Machine-readable cut metadata embedded in a cut PR body.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CutMetadata {
	/// Identifier for this cut (for example `v0.22.0`).
	pub cut_id: String,
	/// Source branch the cut was created from (for example `main`).
	pub base_branch: String,
	/// Frozen base branch that the cut PR targets (for example `release/v0.22.0`).
	pub cut_base_branch: String,
	/// Head branch carrying the release commit.
	pub head_branch: String,
	/// Commit SHA on `base_branch` that `cut_base_branch` was frozen at.
	pub cut_base_commit: String,
	/// Optional release commit SHA on `head_branch`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub release_commit: Option<String>,
	/// Target ids included in this cut.
	#[serde(default)]
	pub targets: Vec<String>,
}

/// Inputs used to render the sticky cut status banner inside a cut PR body.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CutStatusBanner {
	/// Identifier for this cut (for example `v0.22.0`).
	pub cut_id: String,
	/// Full or abbreviated SHA on `base_branch` that the cut was created from.
	pub cut_base_commit: String,
	/// Name of the source branch (for example `main`).
	pub base_branch: String,
	/// Number of commits on `base_branch` since `cut_base_commit`.
	pub commits_behind: usize,
	/// Pending un-included changeset file paths added on `base_branch` since the cut.
	pub pending_changesets: Vec<String>,
	/// Slash command maintainers post to refresh the cut (for example `/refresh`).
	pub refresh_command: String,
}

/// Derive a deterministic cut identifier from a prepared [`ReleaseManifest`].
#[must_use]
pub fn resolve_cut_id(manifest: &ReleaseManifest) -> String {
	if let Some(primary) = manifest.release_targets.iter().find(|target| {
		target.version_format == VersionFormat::Primary && !target.tag_name.is_empty()
	}) {
		return sanitize_branch_segment(&primary.tag_name);
	}

	if manifest.release_targets.len() == 1
		&& let Some(target) = manifest.release_targets.first()
		&& !target.tag_name.is_empty()
	{
		return sanitize_branch_segment(&target.tag_name);
	}

	if let Some(version) = manifest
		.version
		.as_deref()
		.filter(|version| !version.trim().is_empty())
	{
		return sanitize_branch_segment(&format!("v{}", version.trim()));
	}

	let mut sorted_targets = manifest
		.release_targets
		.iter()
		.map(|target| (target.id.as_str(), target.version.as_str()))
		.collect::<Vec<_>>();
	sorted_targets.sort_unstable();

	let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
	for (id, version) in sorted_targets {
		for byte in id
			.bytes()
			.chain(std::iter::once(b':'))
			.chain(version.bytes())
		{
			hash ^= u64::from(byte);
			hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
		}
	}

	format!("batch-{hash:08x}")
}

/// Render a branch template (for example `release/{{ cut_id }}`) with cut variables.
#[must_use]
pub fn render_cut_branch_name(
	template: &str,
	cut_id: &str,
	version: Option<&str>,
	tag: Option<&str>,
) -> String {
	let tag_value = tag.unwrap_or(cut_id);
	let version_value = version
		.map(str::trim)
		.filter(|value| !value.is_empty())
		.unwrap_or_else(|| cut_id.strip_prefix('v').unwrap_or(cut_id));
	let parsed_semver = semver::Version::parse(version_value).ok();
	let major = parsed_semver
		.as_ref()
		.map_or_else(|| "0".to_string(), |semver| semver.major.to_string());
	let minor = parsed_semver
		.as_ref()
		.map_or_else(|| "0".to_string(), |semver| semver.minor.to_string());

	let rendered = template
		.replace("{{ cut_id }}", cut_id)
		.replace("{{cut_id}}", cut_id)
		.replace("{{ tag }}", tag_value)
		.replace("{{tag}}", tag_value)
		.replace("{{ version }}", version_value)
		.replace("{{version}}", version_value)
		.replace("{{ major }}", &major)
		.replace("{{major}}", &major)
		.replace("{{ minor }}", &minor)
		.replace("{{minor}}", &minor);

	rendered
		.split('/')
		.map(sanitize_branch_segment)
		.filter(|segment| !segment.is_empty())
		.collect::<Vec<_>>()
		.join("/")
}

/// Resolve the cut base and head branch names for a release manifest and source config.
#[must_use]
pub fn resolve_cut_branches(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> ResolvedCutBranches {
	let cut_id = resolve_cut_id(manifest);
	let primary_tag = manifest
		.release_targets
		.iter()
		.find(|target| {
			target.version_format == VersionFormat::Primary && !target.tag_name.is_empty()
		})
		.or_else(|| manifest.release_targets.first())
		.map(|target| target.tag_name.as_str())
		.filter(|tag| !tag.is_empty());
	let cut_base_branch = render_cut_branch_name(
		&source.pull_requests.cut.branch_template,
		&cut_id,
		manifest.version.as_deref(),
		primary_tag,
	);
	let prefix = source.pull_requests.branch_prefix.trim_end_matches('/');
	let head_suffix = if source.pull_requests.cut.branch_template.contains("{{") {
		cut_id.clone()
	} else {
		cut_base_branch
			.rsplit_once('/')
			.map_or_else(|| cut_base_branch.clone(), |(_, suffix)| suffix.to_string())
	};
	let head_branch = format!("{prefix}/{head_suffix}");

	ResolvedCutBranches {
		cut_id,
		source_base_branch: source.pull_requests.base.clone(),
		cut_base_branch,
		head_branch,
	}
}

/// Render a release or sync PR title template with `cut_id`, `version`, `tag`, and `base`.
#[must_use]
pub fn render_cut_or_sync_title(
	template: &str,
	cut_id: &str,
	base: &str,
	version: Option<&str>,
) -> String {
	let version_value = version
		.map(str::trim)
		.filter(|value| !value.is_empty())
		.unwrap_or_else(|| cut_id.strip_prefix('v').unwrap_or(cut_id));

	template
		.replace("{{ cut_id }}", cut_id)
		.replace("{{cut_id}}", cut_id)
		.replace("{{ tag }}", cut_id)
		.replace("{{tag}}", cut_id)
		.replace("{{ version }}", version_value)
		.replace("{{version}}", version_value)
		.replace("{{ base }}", base)
		.replace("{{base}}", base)
}

/// Render the embedded machine-readable cut metadata comment block.
#[must_use]
pub fn render_cut_metadata_comment(metadata: &CutMetadata) -> String {
	let encoded = serde_json::to_string(metadata).unwrap_or_else(|_| "{}".to_string());
	format!("{CUT_META_START} {encoded} {CUT_META_END}")
}

/// Parse the embedded machine-readable cut metadata block from a PR body.
#[must_use]
pub fn parse_cut_metadata_comment(body: &str) -> Option<CutMetadata> {
	let start = body.find(CUT_META_START)? + CUT_META_START.len();
	let rest = body.get(start..)?;
	let end = rest.find(CUT_META_END)?;
	let json = rest.get(..end)?.trim();
	serde_json::from_str(json).ok()
}

/// Insert or replace the embedded cut metadata block in a PR body.
#[must_use]
pub fn upsert_cut_metadata_comment(body: &str, metadata: &CutMetadata) -> String {
	let rendered = render_cut_metadata_comment(metadata);

	if let Some(start) = body.find(CUT_META_START)
		&& let Some(rest) = body.get(start..)
		&& let Some(end_offset) = rest.find(CUT_META_END)
	{
		let end = start + end_offset + CUT_META_END.len();
		let mut updated = String::with_capacity(body.len() + rendered.len());
		if let Some(before) = body.get(..start) {
			updated.push_str(before);
		}
		updated.push_str(&rendered);
		if let Some(after) = body.get(end..) {
			updated.push_str(after);
		}
		return updated;
	}

	let trimmed = body.trim_end();
	if trimmed.is_empty() {
		rendered
	} else {
		format!("{trimmed}\n\n{rendered}")
	}
}

/// Render the sticky cut status banner block with HTML comment delimiters.
#[must_use]
pub fn render_cut_status_banner(banner: &CutStatusBanner) -> String {
	let short_sha = banner
		.cut_base_commit
		.get(..7)
		.unwrap_or(&banner.cut_base_commit);
	let mut out = String::new();
	out.push_str(CUT_STATUS_START);
	out.push('\n');

	if banner.commits_behind == 0 {
		let _ = writeln!(
			out,
			"> **Frozen release cut `{}`** from `{}` (`{short_sha}`) — up to date with `{}`.",
			banner.cut_id, banner.base_branch, banner.base_branch
		);
		let _ = writeln!(
			out,
			"> Merge this PR once checks pass to tag and publish immediately without pausing `{}`.",
			banner.base_branch
		);
	} else {
		let commit_word = if banner.commits_behind == 1 {
			"commit"
		} else {
			"commits"
		};
		let changeset_count = banner.pending_changesets.len();
		let changeset_word = if changeset_count == 1 {
			"changeset"
		} else {
			"changesets"
		};
		let _ = writeln!(
			out,
			"> **Frozen release cut `{}`** from `{}` (`{short_sha}`) · **{} newer {commit_word}** ({changeset_count} {changeset_word}) on `{}` since this cut.",
			banner.cut_id, banner.base_branch, banner.commits_behind, banner.base_branch
		);
		let _ = writeln!(
			out,
			"> Merge as-is to ship `{}` now, or comment `{}` to re-cut from the latest green commit on `{}`.",
			banner.cut_id, banner.refresh_command, banner.base_branch
		);

		if !banner.pending_changesets.is_empty() {
			out.push_str(">\n> Newer un-included changesets:\n");
			for path in banner.pending_changesets.iter().take(10) {
				let _ = writeln!(out, "> - `{path}`");
			}
			if banner.pending_changesets.len() > 10 {
				let remaining = banner.pending_changesets.len() - 10;
				let _ = writeln!(out, "> - …and {remaining} more");
			}
		}
	}

	out.push_str(CUT_STATUS_END);
	out
}

/// Insert or replace the sticky cut status banner block inside a PR body.
#[must_use]
pub fn upsert_cut_status_banner(body: &str, banner_block: &str) -> String {
	if let Some(start) = body.find(CUT_STATUS_START)
		&& let Some(rest) = body.get(start..)
		&& let Some(end_offset) = rest.find(CUT_STATUS_END)
	{
		let end = start + end_offset + CUT_STATUS_END.len();
		let mut updated = String::with_capacity(body.len() + banner_block.len());
		if let Some(before) = body.get(..start) {
			updated.push_str(before);
		}
		updated.push_str(banner_block);
		if let Some(after) = body.get(end..) {
			updated.push_str(after);
		}
		return updated;
	}

	let trimmed = body.trim_start();
	if trimmed.is_empty() {
		banner_block.to_string()
	} else {
		format!("{banner_block}\n\n{trimmed}")
	}
}

fn sanitize_branch_segment(segment: &str) -> String {
	segment
		.trim()
		.chars()
		.map(|character| {
			if character.is_ascii_alphanumeric()
				|| character == '.'
				|| character == '-'
				|| character == '_'
			{
				character
			} else {
				'-'
			}
		})
		.collect::<String>()
		.trim_matches('-')
		.to_string()
}

#[cfg(test)]
#[path = "__tests__/cut_tests.rs"]
mod tests;
