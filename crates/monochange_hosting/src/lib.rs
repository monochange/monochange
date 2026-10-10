#![doc(
	html_logo_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/logo-512.png",
	html_favicon_url = "https://raw.githubusercontent.com/monochange/monochange/main/assets/favicon.ico"
)]
#![forbid(clippy::indexing_slicing)]
#![doc = include_str!("crate_docs.md")]
use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use monochange_core::CommitMessage;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ProviderPullRequestBodyStyle;
use monochange_core::ProviderReleaseNotesSource;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestChangelog;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseOwnerKind;
use monochange_core::SourceChangeRequestBodyTruncation;
use monochange_core::SourceConfiguration;
use monochange_core::git::git_checkout_branch_command;
use monochange_core::git::git_current_branch;
use monochange_core::git::git_push_branch_command;
use monochange_core::git::git_stage_all_command;
use monochange_core::git::git_stage_paths_command;
use monochange_core::git::run_command;
use monochange_core::git::run_git_commit_message;
use reqwest::Client;
use reqwest::header::HeaderMap;
use rustls::crypto::ring::default_provider as ring_provider;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Default total timeout for provider API requests.
///
/// Provider-backed release and pull-request commands should fail with context
/// rather than appear hung forever when a non-GitHub host or network path stalls.
pub const PROVIDER_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Default connection timeout for provider API requests.
///
/// A shorter connect timeout catches unreachable self-hosted GitLab, Gitea, and
/// Forgejo instances before the user is left waiting without feedback.
pub const PROVIDER_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

static RUSTLS_PROVIDER_INSTALLED: OnceLock<()> = OnceLock::new();

/// Install the ring crypto provider as the default for rustls.
///
/// Required because monochange uses `reqwest` with the `rustls-no-provider` feature —
/// without an explicit provider, any HTTPS request panics with "No provider set".
///
/// Safe to call multiple times; subsequent calls are no-ops.
pub fn ensure_rustls_provider() {
	let () = RUSTLS_PROVIDER_INSTALLED.get_or_init(|| {
		let _ = ring_provider().install_default();
	});
}

/// Append release-note entries to a markdown body, normalizing bullet formatting.
pub fn push_body_entries(lines: &mut Vec<String>, entries: &[String]) {
	for (index, entry) in entries.iter().enumerate() {
		let trimmed = entry.trim();

		if trimmed.contains('\n') {
			lines.extend(trimmed.lines().map(ToString::to_string));
			if index + 1 < entries.len() {
				lines.push(String::new());
			}
			continue;
		}

		if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with('#') {
			lines.push(trimmed.to_string());
		} else {
			lines.push(format!("- {trimmed}"));
		}
	}
}

/// Render a fallback release body when no changelog body is available.
pub fn minimal_release_body(manifest: &ReleaseManifest, target: &ReleaseManifestTarget) -> String {
	let mut lines = vec![format!("Release target `{}`", target.id), String::new()];

	if !target.members.is_empty() {
		lines.push(format!("Members: {}", target.members.join(", ")));
		lines.push(String::new());
	}

	let reasons = manifest
		.plan
		.decisions
		.iter()
		.filter(|decision| {
			target.kind == ReleaseOwnerKind::Package || target.members.contains(&decision.package)
		})
		.flat_map(|decision| decision.reasons.iter().cloned())
		.collect::<Vec<_>>();

	if reasons.is_empty() {
		lines.push("- prepare release".to_string());
	} else {
		for reason in reasons {
			lines.push(format!("- {reason}"));
		}
	}

	lines.join("\n")
}

/// Build the provider change-request branch for a release command.
///
/// Built-in `step <name>` commands run one step outside any configured
/// workflow, so they share the default `release` branch. Deriving the branch
/// from the step name would put each step on its own branch, e.g.
/// `monochange/release/step-open-release-request`.
pub fn release_pull_request_branch(branch_prefix: &str, command: &str) -> String {
	let command = if command.starts_with("step ") {
		"release"
	} else {
		command
	};
	let command = command
		.chars()
		.map(|character| {
			if character.is_ascii_alphanumeric() {
				character.to_ascii_lowercase()
			} else {
				'-'
			}
		})
		.collect::<String>()
		.trim_matches('-')
		.to_string();

	let command = if command.is_empty() {
		"release".to_string()
	} else {
		command
	};

	format!("{}/{}", branch_prefix.trim_end_matches('/'), command)
}

/// Render the markdown body used for provider release requests.
pub fn release_pull_request_body(manifest: &ReleaseManifest) -> String {
	render_release_pull_request_body(manifest, ProviderPullRequestBodyStyle::Full, None).body
}

/// Render the release request body using the configured style and provider limit.
pub fn release_pull_request_body_for_source(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> RenderedReleasePullRequestBody {
	render_release_pull_request_body(
		manifest,
		source.pull_requests.body_style,
		source
			.pull_requests
			.effective_max_body_chars(source.provider),
	)
}

/// Render the markdown body used for provider release requests, bounded to a provider limit.
///
/// The prepared-release header and the outward target list always survive. When
/// the body would exceed `max_chars`, release-note sections are dropped from the
/// end until it fits and a pointer to the changelog files that still carry every
/// note is appended, so a shortened body never reads as complete. A body that
/// already fits renders exactly as before, with no added pointer.
///
/// [`ProviderPullRequestBodyStyle::Summary`] skips the notes entirely and always
/// renders the pointer, leaving them to the changelog files and hosted release body.
pub fn render_release_pull_request_body(
	manifest: &ReleaseManifest,
	style: ProviderPullRequestBodyStyle,
	max_chars: Option<usize>,
) -> RenderedReleasePullRequestBody {
	let header = release_pull_request_header(manifest);
	let summary_style = matches!(style, ProviderPullRequestBodyStyle::Summary);
	let sections = if summary_style {
		Vec::new()
	} else {
		release_pull_request_sections(manifest)
	};
	let original_chars =
		assemble_release_pull_request_body(&header, &sections, sections.len(), None, manifest)
			.chars()
			.count();

	let mut kept = sections.len();
	if let Some(max_chars) = max_chars {
		while kept > 0
			&& assemble_release_pull_request_body(
				&header,
				&sections,
				kept,
				Some(dropped_release_note_entries(&sections, kept)),
				manifest,
			)
			.chars()
			.count() > max_chars
		{
			kept -= 1;
		}
	}

	let truncated = kept < sections.len();
	let dropped_entries = dropped_release_note_entries(&sections, kept);
	// The pointer is added when content was dropped, or when the summary style
	// deliberately omits the notes. An unbounded full-style body is never
	// altered, so existing output is preserved byte-for-byte.
	let shows_pointer = truncated || summary_style;
	let body = assemble_release_pull_request_body(
		&header,
		&sections,
		kept,
		shows_pointer.then_some(dropped_entries),
		manifest,
	);
	let truncation = truncated.then(|| {
		SourceChangeRequestBodyTruncation {
			max_chars: max_chars.unwrap_or_default(),
			original_chars,
			dropped_entries,
		}
	});
	RenderedReleasePullRequestBody { body, truncation }
}

/// A rendered release request body and, when it was shortened, what was lost.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RenderedReleasePullRequestBody {
	pub body: String,
	/// Set when the body was shortened to stay inside the provider limit.
	pub truncation: Option<SourceChangeRequestBodyTruncation>,
}

/// One droppable span of release-request content.
struct ReleaseBodySection {
	text: String,
	/// Release-note entries the section carries, reported when it is dropped.
	entries: usize,
	/// Whether the section renders under the `## Release notes` heading.
	notes: bool,
	/// Release target the section belongs to, so a kept target heading is never
	/// left without the notes that explain it.
	group: usize,
	/// Whether the section is only a target heading and, without summary
	/// paragraphs of its own, reads as empty when its group is dropped.
	heading_only: bool,
}

/// Render the prepared-release header and the outward target list.
fn release_pull_request_header(manifest: &ReleaseManifest) -> Vec<String> {
	let mut lines = vec!["## Prepared release".to_string(), String::new()];
	lines.push(format!("- command: `{}`", manifest.command));

	for target in manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
	{
		lines.push(format!(
			"- {} `{}` -> `{}`",
			target.kind, target.id, target.tag_name
		));
	}

	if !manifest.release_targets.iter().any(|target| target.release) {
		lines.push("- no outward release targets".to_string());
	}

	lines
}

/// Render every droppable span of release content, in body order.
fn release_pull_request_sections(manifest: &ReleaseManifest) -> Vec<ReleaseBodySection> {
	let mut sections = Vec::new();
	for (group, target) in manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
		.enumerate()
	{
		let Some(changelog) = manifest.changelogs.iter().find(|changelog| {
			changelog.owner_id == target.id && changelog.owner_kind == target.kind
		}) else {
			let mut lines = vec![format!("### {} {}", target.id, target.version)];
			lines.push(String::new());
			lines.push(minimal_release_body(manifest, target));
			sections.push(ReleaseBodySection {
				text: lines.join("\n"),
				entries: 1,
				notes: true,
				group,
				heading_only: false,
			});
			continue;
		};

		let mut intro = vec![format!("### {} {}", target.id, target.version)];
		for paragraph in &changelog.notes.summary {
			intro.push(String::new());
			intro.push(paragraph.clone());
		}
		sections.push(ReleaseBodySection {
			text: intro.join("\n"),
			entries: 0,
			notes: true,
			group,
			heading_only: changelog.notes.summary.is_empty(),
		});

		for section in &changelog.notes.sections {
			if section.entries.is_empty() {
				continue;
			}
			let mut lines = vec![format!("### {}", section.title), String::new()];
			push_body_entries(&mut lines, &section.entries);
			sections.push(ReleaseBodySection {
				text: lines.join("\n"),
				entries: section.entries.len(),
				notes: true,
				group,
				heading_only: false,
			});
		}
	}

	if !manifest.changed_files.is_empty() {
		let mut lines = vec!["## Changed files".to_string(), String::new()];
		for path in &manifest.changed_files {
			lines.push(format!("- {}", path.display()));
		}
		sections.push(ReleaseBodySection {
			text: lines.join("\n"),
			entries: 0,
			notes: false,
			group: usize::MAX,
			heading_only: false,
		});
	}

	sections
}

/// Join the header, the first `kept` sections, and an optional changelog pointer.
fn assemble_release_pull_request_body(
	header: &[String],
	sections: &[ReleaseBodySection],
	kept: usize,
	pointer: Option<usize>,
	manifest: &ReleaseManifest,
) -> String {
	let mut lines = header.to_vec();
	let kept = sections.iter().take(kept).collect::<Vec<_>>();
	let filled_groups = kept
		.iter()
		.filter(|section| !section.heading_only)
		.map(|section| section.group)
		.collect::<Vec<_>>();
	let notes = kept
		.iter()
		.filter(|section| section.notes)
		.filter(|section| !section.heading_only || filled_groups.contains(&section.group))
		.collect::<Vec<_>>();
	// Without a pointer the body is the unbounded render, which keeps the
	// `## Release notes` heading even when a release has no outward targets.
	if !notes.is_empty() || pointer.is_none() {
		lines.push(String::new());
		lines.push("## Release notes".to_string());
		for section in notes {
			lines.push(String::new());
			lines.push(section.text.clone());
		}
	}
	for section in kept.iter().filter(|section| !section.notes) {
		lines.push(String::new());
		lines.push(section.text.clone());
	}
	if let Some(dropped_entries) = pointer {
		push_release_pull_request_changelog_pointer(&mut lines, manifest, dropped_entries);
	}
	lines.join("\n")
}

/// Append a pointer to the changelog files that carry the complete release notes.
///
/// The notice stays short on purpose: it is appended to every shortened body, so
/// its length is the floor the truncation loop cannot go below.
fn push_release_pull_request_changelog_pointer(
	lines: &mut Vec<String>,
	manifest: &ReleaseManifest,
	dropped_entries: usize,
) {
	lines.push(String::new());
	lines.push("## Full release notes".to_string());
	lines.push(String::new());
	if dropped_entries > 0 {
		lines.push(format!(
			"{dropped_entries} entries omitted to fit the body limit. The complete notes are in:"
		));
	} else {
		lines.push("The notes are omitted here. Find them in:".to_string());
	}

	let paths = release_pull_request_changelog_paths(manifest);
	if paths.is_empty() {
		lines.push(String::new());
		lines.push("Run `monochange notes --output <id>` to read them.".to_string());
		return;
	}

	lines.push(String::new());
	for path in paths {
		lines.push(format!("- `{path}`"));
	}
}

/// Collect the changelog paths for outward release targets, in target order.
fn release_pull_request_changelog_paths(manifest: &ReleaseManifest) -> Vec<String> {
	let mut paths = Vec::new();
	for target in manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
	{
		let Some(changelog) = manifest.changelogs.iter().find(|changelog| {
			changelog.owner_id == target.id && changelog.owner_kind == target.kind
		}) else {
			continue;
		};
		let path = changelog.path.display().to_string();
		if !paths.contains(&path) {
			paths.push(path);
		}
	}
	paths
}

/// Count the release-note entries carried by the sections after `kept`.
fn dropped_release_note_entries(sections: &[ReleaseBodySection], kept: usize) -> usize {
	sections
		.iter()
		.skip(kept)
		.map(|section| section.entries)
		.sum()
}

/// Resolve the provider release body for one outward release target.
pub fn release_body(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
	target: &ReleaseManifestTarget,
) -> Option<String> {
	match source.releases.source {
		ProviderReleaseNotesSource::GitHubGenerated => None,
		ProviderReleaseNotesSource::Monochange => {
			Some(monochange_release_body(
				manifest,
				target,
				&source.releases.changelog_output,
			))
		}
	}
}

fn monochange_release_body(
	manifest: &ReleaseManifest,
	target: &ReleaseManifestTarget,
	output: &str,
) -> String {
	let target_changelog = manifest.changelogs.iter().find(|changelog| {
		changelog.owner_id == target.id
			&& changelog.owner_kind == target.kind
			&& changelog.output == output
	});

	match target_changelog {
		Some(changelog) if changelog_has_release_notes(changelog) => {
			provider_body_from_changelog_rendered(&changelog.rendered)
		}
		_ => minimal_release_body(manifest, target),
	}
}

/// Convert a changelog file section into a provider release body.
///
/// Changelog files keep the version title as `## <title>` with sections as
/// `### <section>` and expanded entries as `#### <entry>`. Provider releases
/// already carry that title in the release `name` (which includes the date by
/// default), so the body drops the title header and promotes one level to
/// match knope-style releases where the body starts at `## <section>` with
/// expanded entries as `### <entry>`.
fn provider_body_from_changelog_rendered(rendered: &str) -> String {
	let mut lines = rendered.lines().collect::<Vec<_>>();
	if lines.first().is_some_and(|first| first.starts_with("## ")) {
		lines.remove(0);
		while lines.first().is_some_and(|line| line.trim().is_empty()) {
			lines.remove(0);
		}
	}
	let mut promoted = Vec::with_capacity(lines.len());
	for line in lines {
		if let Some(rest) = line.strip_prefix("#### ") {
			promoted.push(format!("### {rest}"));
		} else if let Some(rest) = line.strip_prefix("### ") {
			promoted.push(format!("## {rest}"));
		} else {
			promoted.push(line.to_string());
		}
	}
	promoted.join("\n")
}

fn changelog_has_release_notes(changelog: &ReleaseManifestChangelog) -> bool {
	changelog.notes.sections.iter().any(|section| {
		section
			.entries
			.iter()
			.any(|entry| !is_empty_release_note(entry))
	})
}

fn is_empty_release_note(entry: &str) -> bool {
	entry.contains("No group-facing notes were recorded for this release")
		|| entry.contains("No package-specific changes were recorded")
		|| entry.contains("No significant changes")
}

/// Build a blocking HTTP client for provider API calls.
///
/// Build a blocking HTTP client for provider API calls.
///
/// Installs the ring crypto provider for rustls if not already set, ensuring
/// HTTPS works with the `rustls-no-provider` feature flag.
pub fn build_http_client(provider: &str) -> MonochangeResult<Client> {
	ensure_rustls_provider();

	Client::builder()
		.connect_timeout(PROVIDER_HTTP_CONNECT_TIMEOUT)
		.timeout(PROVIDER_HTTP_TIMEOUT)
		.build()
		// patch-coverage:ignore-start -- reqwest client construction failures are platform/TLS configuration dependent.
		.map_err(|error| http_client_build_error(provider, error))
	// patch-coverage:ignore-end
}

fn http_client_build_error(provider: &str, error: impl Display) -> MonochangeError {
	MonochangeError::Config(format!("failed to build {provider} HTTP client: {error}"))
}

/// Perform a GET request that treats `404` as `Ok(None)`.
pub async fn get_optional_json<T>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	provider: &str,
) -> MonochangeResult<Option<T>>
where
	T: DeserializeOwned,
{
	let response = client
		.get(url)
		.headers(headers.clone())
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
		})?;
	if response.status().as_u16() == 404 {
		return Ok(None);
	}
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API GET `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<T>().await.map(Some).map_err(|error| {
		MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
	})
}

/// Perform a GET request and deserialize a successful JSON response.
pub async fn get_json<T>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	provider: &str,
) -> MonochangeResult<T>
where
	T: DeserializeOwned,
{
	let response = client
		.get(url)
		.headers(headers.clone())
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API GET `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<T>().await.map_err(|error| {
		MonochangeError::Config(format!("{provider} API GET `{url}` failed: {error}"))
	})
}

/// Perform a POST request and deserialize a successful JSON response.
pub async fn post_json<Body, Response>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	body: &Body,
	provider: &str,
) -> MonochangeResult<Response>
where
	Body: Serialize + ?Sized,
	Response: DeserializeOwned,
{
	let response = client
		.post(url)
		.headers(headers.clone())
		.json(body)
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API POST `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API POST `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().await.map_err(|error| {
		MonochangeError::Config(format!("{provider} API POST `{url}` failed: {error}"))
	})
}

/// Perform a PUT request and deserialize a successful JSON response.
pub async fn put_json<Body, Response>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	body: &Body,
	provider: &str,
) -> MonochangeResult<Response>
where
	Body: Serialize + ?Sized,
	Response: DeserializeOwned,
{
	let response = client
		.put(url)
		.headers(headers.clone())
		.json(body)
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API PUT `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API PUT `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().await.map_err(|error| {
		MonochangeError::Config(format!("{provider} API PUT `{url}` failed: {error}"))
	})
}

/// Perform a PATCH request and deserialize a successful JSON response.
pub async fn patch_json<Body, Response>(
	client: &Client,
	headers: &HeaderMap,
	url: &str,
	body: &Body,
	provider: &str,
) -> MonochangeResult<Response>
where
	Body: Serialize + ?Sized,
	Response: DeserializeOwned,
{
	let response = client
		.patch(url)
		.headers(headers.clone())
		.json(body)
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("{provider} API PATCH `{url}` failed: {error}"))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"{provider} API PATCH `{url}` failed with status {}",
			response.status()
		)));
	}
	response.json::<Response>().await.map_err(|error| {
		MonochangeError::Config(format!("{provider} API PATCH `{url}` failed: {error}"))
	})
}

/// Check out or reset the local release branch used for provider requests.
pub async fn git_checkout_branch(root: &Path, branch: &str, context: &str) -> MonochangeResult<()> {
	if matches!(git_current_branch(root).await.as_deref(), Ok(current) if current == branch) {
		return Ok(());
	}
	run_command(git_checkout_branch_command(root, branch), context).await
}

/// Stage every non-ignored changed path before creating a release commit.
pub async fn git_stage_paths(
	root: &Path,
	tracked_paths: &[PathBuf],
	context: &str,
	stage_all: bool,
) -> MonochangeResult<()> {
	// patch-coverage:ignore-start -- provider integration tests cover the staged command choice through adapters.
	let command = if stage_all {
		git_stage_all_command(root)
	} else {
		git_stage_paths_command(root, tracked_paths)
	};
	// patch-coverage:ignore-end
	run_command(command, context).await
}

/// Commit the prepared release changes, tolerating a no-op commit.
pub async fn git_commit_paths(
	root: &Path,
	message: &CommitMessage,
	context: &str,
	no_verify: bool,
) -> MonochangeResult<()> {
	run_git_commit_message(root, message, context, no_verify).await
}

/// Push the release branch to `origin` with `--force-with-lease`.
pub async fn git_push_branch(
	root: &Path,
	branch: &str,
	context: &str,
	no_verify: bool,
) -> MonochangeResult<()> {
	run_command(git_push_branch_command(root, branch, no_verify), context).await
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
