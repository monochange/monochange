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
use monochange_core::ProviderReleaseNotesSource;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestChangelog;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseOwnerKind;
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
pub fn release_pull_request_branch(branch_prefix: &str, command: &str) -> String {
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

	lines.push(String::new());
	lines.push("## Release notes".to_string());

	for target in manifest
		.release_targets
		.iter()
		.filter(|target| target.release)
	{
		lines.push(String::new());
		lines.push(format!("### {} {}", target.id, target.version));

		if let Some(changelog) = manifest.changelogs.iter().find(|changelog| {
			changelog.owner_id == target.id && changelog.owner_kind == target.kind
		}) {
			for paragraph in &changelog.notes.summary {
				lines.push(String::new());
				lines.push(paragraph.clone());
			}

			for section in &changelog.notes.sections {
				if section.entries.is_empty() {
					continue;
				}
				lines.push(String::new());
				lines.push(format!("### {}", section.title));
				lines.push(String::new());
				push_body_entries(&mut lines, &section.entries);
			}
		} else {
			lines.push(String::new());
			lines.push(minimal_release_body(manifest, target));
		}
	}

	if !manifest.changed_files.is_empty() {
		lines.push(String::new());
		lines.push("## Changed files".to_string());
		lines.push(String::new());

		for path in &manifest.changed_files {
			lines.push(format!("- {}", path.display()));
		}
	}

	lines.join("\n")
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
	let member_changelogs = uncovered_member_changelogs(manifest, target, target_changelog, output);

	match (target_changelog, member_changelogs.is_empty()) {
		(Some(changelog), _) if changelog_has_release_notes(changelog) => {
			changelog.rendered.clone()
		}
		(_, false) => grouped_member_release_body(target, &member_changelogs),
		(_, true) => minimal_release_body(manifest, target),
	}
}

fn uncovered_member_changelogs<'a>(
	manifest: &'a ReleaseManifest,
	target: &ReleaseManifestTarget,
	target_changelog: Option<&ReleaseManifestChangelog>,
	output: &str,
) -> Vec<&'a ReleaseManifestChangelog> {
	if target.kind != ReleaseOwnerKind::Group {
		return Vec::new();
	}

	manifest
		.changelogs
		.iter()
		.filter(|changelog| {
			changelog.owner_kind == ReleaseOwnerKind::Package
				&& target.members.contains(&changelog.owner_id)
				&& changelog.output == output
				&& changelog_has_release_notes(changelog)
				&& changelog_has_uncovered_notes(changelog, target_changelog)
		})
		.collect()
}

fn grouped_member_release_body(
	target: &ReleaseManifestTarget,
	member_changelogs: &[&ReleaseManifestChangelog],
) -> String {
	let title = if target.rendered_changelog_title.is_empty() {
		target.rendered_title.as_str()
	} else {
		target.rendered_changelog_title.as_str()
	};
	let title = if title.is_empty() {
		target.tag_name.as_str()
	} else {
		title
	};
	let mut lines = vec![format!("## {title}"), String::new()];
	lines.push(format!("Grouped release for `{}`.", target.id));
	lines.push(String::new());
	push_member_changelogs(&mut lines, member_changelogs);
	lines.join("\n")
}

fn push_member_changelogs(lines: &mut Vec<String>, changelogs: &[&ReleaseManifestChangelog]) {
	lines.push("## Member package changelogs".to_string());

	for changelog in changelogs {
		lines.push(String::new());
		lines.push(format!("### `{}`", changelog.owner_id));
		push_changelog_notes(lines, changelog);
	}
}

fn push_changelog_notes(lines: &mut Vec<String>, changelog: &ReleaseManifestChangelog) {
	for paragraph in &changelog.notes.summary {
		lines.push(String::new());
		lines.push(paragraph.clone());
	}

	for section in &changelog.notes.sections {
		let entries = section
			.entries
			.iter()
			.filter(|entry| !is_empty_release_note(entry))
			.cloned()
			.collect::<Vec<_>>();
		if entries.is_empty() {
			continue;
		}
		lines.push(String::new());
		lines.push(format!("#### {}", section.title));
		lines.push(String::new());
		push_body_entries(lines, &entries);
	}
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

fn changelog_has_uncovered_notes(
	changelog: &ReleaseManifestChangelog,
	target_changelog: Option<&ReleaseManifestChangelog>,
) -> bool {
	let Some(target_changelog) = target_changelog else {
		return true;
	};
	let covered_entries = target_changelog
		.notes
		.sections
		.iter()
		.flat_map(|section| &section.entries)
		.map(|entry| normalized_release_entry(entry))
		.collect::<Vec<_>>();

	changelog
		.notes
		.sections
		.iter()
		.flat_map(|section| &section.entries)
		.filter(|entry| !is_empty_release_note(entry))
		.any(|entry| !covered_entries.contains(&normalized_release_entry(entry)))
}

fn normalized_release_entry(entry: &str) -> String {
	entry
		.lines()
		.filter(|line| !line.trim_start().starts_with("_Packages:"))
		.collect::<Vec<_>>()
		.join("\n")
		.trim()
		.to_string()
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
