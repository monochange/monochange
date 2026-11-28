//! Frozen release cut helpers: unsynced tag discovery, changelog carry-forward,
//! sticky status banner computation, and conflict-free post-release sync (`SyncRelease`).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use monochange_changelog::changelog_contains_version;
use monochange_changelog::insert_changelog_section;
use monochange_core::BumpSeverity;
use monochange_core::CommitMessage;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ReleaseDecision;
use monochange_core::ReleasePlan;
use monochange_core::ReleaseRecord;
use monochange_core::SourceChangeRequest;
use monochange_core::SourceConfiguration;
use monochange_core::SyncMode;
use monochange_core::WorkspaceConfiguration;
use monochange_core::parse_release_record_json;
use monochange_core::relative_to_root;
use monochange_hosting::cut::CutMetadata;
use monochange_hosting::cut::CutStatusBanner;
use monochange_hosting::cut::render_cut_or_sync_title;
use monochange_hosting::cut::render_cut_status_banner;
use serde::Deserialize;
use serde::Serialize;

use crate::FileUpdate;
use crate::OutputFormat;
use crate::git_support::git_added_changesets_between;
use crate::git_support::git_commit_count_between;
use crate::git_support::git_commit_paths;
use crate::git_support::git_create_bridge_commit;
use crate::git_support::git_head_commit;
use crate::git_support::git_merged_tags;
use crate::git_support::git_stage_all;
use crate::git_support::git_stage_paths;
use crate::release_artifacts::ReleasePaths;
use crate::release_artifacts::build_manifest_updates_parallel;
use crate::release_artifacts::load_sorted_tags;
use crate::release_record::discover_release_record;
use crate::versioned_files::build_versioned_file_updates_with_base_updates;
use crate::versioning_state::ResolvedReleaseValues;
use crate::workspace_ops::build_dependency_sync_updates;
use crate::workspace_ops::discover_release_workspace;
use crate::workspace_ops::merge_file_updates;

/// A published release tag discovered outside `HEAD`'s synced history.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PublishedReleaseRecord {
	pub tag_name: String,
	pub commit: String,
	pub record_path: PathBuf,
	pub record: ReleaseRecord,
}

/// Structured output returned by `SyncRelease`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct SyncReleaseReport {
	pub mode: SyncMode,
	pub status: String,
	pub dry_run: bool,
	pub cut_id: Option<String>,
	pub synced_tags: Vec<String>,
	pub synced_commits: Vec<String>,
	pub deleted_changesets: Vec<PathBuf>,
	pub updated_changelogs: Vec<PathBuf>,
	pub synced_record_paths: Vec<PathBuf>,
	pub updated_manifests: Vec<PathBuf>,
	pub changed_files: Vec<PathBuf>,
}

/// Ephemeral cut execution state threaded across `PrepareRelease`,
/// `CommitRelease`, and `OpenReleaseRequest` within one CLI command run.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub(crate) struct CutExecutionContext {
	/// Resolved base commit (`M*`) on `[source.pull_requests].base` for this cut.
	pub cut_base_commit: Option<String>,
	/// Whether an active open cut PR lookup has already been attempted in this run.
	pub active_cut_pr_checked: bool,
	/// Active open cut PR discovered before preparing, if any.
	pub active_cut_pr: Option<monochange_core::ActiveCutPullRequest>,
	/// When sticky mode skips re-cutting, holds the precomputed body update for
	/// `OpenReleaseRequest` to patch onto `active_cut_pr`.
	pub sticky_banner_body: Option<String>,
}

/// Collect published release tags whose release commits have not yet been synced onto `HEAD`.
///
/// Uses a single `git tag --merged HEAD` call to skip every tag already reachable
/// from `HEAD`, then checks unmerged tags for durable `ReleaseRecord` commits and
/// skips any record already written to `.monochange/releases/<hash>/release.json`
/// on disk (which marks linear `semantic_pr` / `bookkeeping_only` syncs as done).
pub(crate) async fn collect_unsynced_published_releases(
	root: &Path,
	sorted_tags: &[String],
) -> MonochangeResult<Vec<PublishedReleaseRecord>> {
	if sorted_tags.is_empty() {
		return Ok(Vec::new());
	}

	let merged_tags = git_merged_tags(root, "HEAD").await?;
	let mut seen_commits = BTreeSet::new();
	let mut unsynced = Vec::new();

	for tag in sorted_tags {
		if merged_tags.contains(tag) {
			continue;
		}

		let Ok(discovery) = discover_release_record(root, tag).await else {
			continue;
		};

		if discovery.distance != 0 || discovery.record.synced_from_commit.is_some() {
			continue;
		}

		if !seen_commits.insert(discovery.record_commit.clone()) {
			continue;
		}

		let record_paths = ReleasePaths::from_record(root, &discovery.record);
		let record_relative =
			repository_relative_path(root, &record_paths.relative, "read a release record")?;

		if is_record_already_synced_on_disk(&root.join(&record_relative), &discovery.record_commit)
		{
			continue;
		}

		unsynced.push(PublishedReleaseRecord {
			tag_name: tag.clone(),
			commit: discovery.record_commit,
			record_path: record_relative,
			record: discovery.record,
		});
	}

	unsynced.sort_by(|left, right| {
		left.record
			.created_at
			.cmp(&right.record.created_at)
			.then_with(|| left.tag_name.cmp(&right.tag_name))
	});

	Ok(unsynced)
}

fn is_record_already_synced_on_disk(record_path: &Path, release_commit: &str) -> bool {
	let Ok(text) = fs::read_to_string(record_path) else {
		return false;
	};
	let Ok(existing) = parse_release_record_json(&text) else {
		return false;
	};

	existing.synced_from_commit.as_deref() == Some(release_commit)
}

/// Collect root-relative paths of changesets already consumed by unsynced published releases.
#[must_use]
pub(crate) fn unsynced_consumed_changeset_paths(
	root: &Path,
	unsynced: &[PublishedReleaseRecord],
) -> BTreeSet<PathBuf> {
	let mut paths = BTreeSet::new();

	for published in unsynced {
		for path in &published.record.deleted_changesets {
			if let Ok(relative) =
				repository_relative_path(root, path, "consume a changeset from a release record")
			{
				paths.insert(relative);
			}
		}
	}

	paths
}

/// Carry forward rendered changelog sections from unsynced published releases so a
/// newly cut release keeps chronological changelog continuity even before `main` syncs.
pub(crate) fn carry_forward_unsynced_changelogs(
	root: &Path,
	unsynced: &[PublishedReleaseRecord],
	changelog_updates: &[monochange_changelog::ChangelogUpdate],
	changelog_file_updates: &mut Vec<FileUpdate>,
) -> MonochangeResult<()> {
	if unsynced.is_empty() {
		return Ok(());
	}

	// Build the base text (disk + unsynced releases oldest-to-newest) for every
	// changelog path touched by any unsynced release.
	let mut carried_by_path = BTreeMap::<PathBuf, String>::new();

	for published in unsynced {
		for entry in &published.record.changelogs {
			if entry.rendered.trim().is_empty() {
				continue;
			}

			let relative = repository_relative_path(root, &entry.path, "read a synced changelog")?;
			let full_path = root.join(relative);
			let version = record_version_for_owner(&published.record, &entry.owner_id);
			let current = match carried_by_path.get(&full_path) {
				Some(existing) => existing.clone(),
				None => read_file_or_empty(&full_path)?,
			};

			if version
				.as_deref()
				.is_some_and(|version| changelog_contains_version(&current, version))
			{
				carried_by_path.entry(full_path).or_insert(current);
				continue;
			}

			let updated = insert_changelog_section(&current, &entry.rendered, None);
			carried_by_path.insert(full_path, updated);
		}
	}

	if carried_by_path.is_empty() {
		return Ok(());
	}

	// Re-apply the current release's newly rendered section on top of the carried
	// base so the resulting file reads: [new release] -> [unsynced releases] -> [disk].
	let mut new_rendered_by_path = BTreeMap::new();

	for update in changelog_updates {
		let relative =
			repository_relative_path(root, &update.file.path, "carry a changelog forward")?;
		new_rendered_by_path.insert(root.join(relative), update);
	}

	let mut updated_paths = BTreeSet::new();

	for file_update in changelog_file_updates.iter_mut() {
		let relative =
			repository_relative_path(root, &file_update.path, "carry a changelog forward")?;
		let full_path = root.join(relative);

		if let Some(carried_base) = carried_by_path.get(&full_path)
			&& let Some(new_update) = new_rendered_by_path.get(&full_path)
		{
			let merged = insert_changelog_section(carried_base, &new_update.rendered, None);
			file_update.content = merged.into_bytes();
			updated_paths.insert(full_path);
		}
	}

	// Also include carried-forward changelogs for targets that were part of an
	// unsynced release but are not changing in the current release.
	for (full_path, carried_text) in carried_by_path {
		if updated_paths.contains(&full_path) {
			continue;
		}

		let disk_text = read_file_or_empty(&full_path)?;

		if carried_text != disk_text {
			changelog_file_updates.push(FileUpdate {
				path: full_path,
				content: carried_text.into_bytes(),
			});
		}
	}

	Ok(())
}

/// Inspect unsynced published tags for the most recent release record's versioning inputs.
pub(crate) async fn latest_unsynced_release_versioning(
	root: &Path,
	sorted_tags: &[String],
) -> Option<(
	Option<monochange_core::versioning::LabelInputs>,
	Option<String>,
	String,
)> {
	let unsynced = collect_unsynced_published_releases(root, sorted_tags)
		.await
		.ok()?;
	let latest = unsynced.into_iter().next_back()?;
	let record = latest.record;
	let inputs = (!record.label_inputs.is_empty()).then_some(record.label_inputs);
	let version = record
		.release_targets
		.first()
		.map(|target| target.version.clone())
		.or(record.version);
	Some((inputs, version, record.created_at))
}

/// Compute and render the sticky status banner for an open cut PR relative to `base_ref`.
pub(crate) async fn compute_cut_status_banner(
	root: &Path,
	metadata: &CutMetadata,
	base_ref: &str,
	refresh_command: &str,
	unsynced_consumed: &BTreeSet<PathBuf>,
) -> MonochangeResult<String> {
	let commits_behind =
		git_commit_count_between(root, &metadata.cut_base_commit, base_ref).await?;
	let added = git_added_changesets_between(root, &metadata.cut_base_commit, base_ref).await?;
	let pending_changesets = added
		.into_iter()
		.filter(|path| !unsynced_consumed.contains(&PathBuf::from(path)))
		.collect();

	Ok(render_cut_status_banner(&CutStatusBanner {
		cut_id: metadata.cut_id.clone(),
		cut_base_commit: metadata.cut_base_commit.clone(),
		base_branch: metadata.base_branch.clone(),
		commits_behind,
		pending_changesets,
		refresh_command: refresh_command.to_string(),
	}))
}

/// Execute `SyncRelease`: replay published cut release bookkeeping onto the current branch.
pub(crate) async fn execute_sync_release(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	from: Option<&str>,
	mode_override: Option<SyncMode>,
	dry_run: bool,
) -> MonochangeResult<SyncReleaseReport> {
	let sync_settings = configuration
		.source
		.as_ref()
		.map(|source| source.pull_requests.sync.clone())
		.unwrap_or_default();
	let mode = mode_override.unwrap_or(sync_settings.mode);

	if mode == SyncMode::None {
		return Ok(SyncReleaseReport {
			mode,
			status: "skipped".to_string(),
			dry_run,
			cut_id: None,
			synced_tags: Vec::new(),
			synced_commits: Vec::new(),
			deleted_changesets: Vec::new(),
			updated_changelogs: Vec::new(),
			synced_record_paths: Vec::new(),
			updated_manifests: Vec::new(),
			changed_files: Vec::new(),
		});
	}

	let sorted_tags = load_sorted_tags(root).await;
	let mut releases = collect_unsynced_published_releases(root, &sorted_tags).await?;

	if let Some(from_ref) = from.map(str::trim).filter(|value| !value.is_empty()) {
		let discovery = discover_release_record(root, from_ref).await?;

		if discovery.distance != 0 {
			return Err(MonochangeError::Config(format!(
				"ref `{from_ref}` resolves to commit {}, but the nearest release record is in ancestor {} at distance {}; `sync-release` requires a release commit or tag",
				crate::short_commit_sha(&discovery.resolved_commit),
				crate::short_commit_sha(&discovery.record_commit),
				discovery.distance
			)));
		}

		if discovery.record.synced_from_commit.is_some() {
			return Err(MonochangeError::Config(format!(
				"ref `{from_ref}` is already a synced bookkeeping record commit"
			)));
		}

		let already_present = releases
			.iter()
			.any(|release| release.commit == discovery.record_commit);

		if !already_present {
			let record_paths = ReleasePaths::from_record(root, &discovery.record);
			releases.push(PublishedReleaseRecord {
				tag_name: from_ref.to_string(),
				commit: discovery.record_commit,
				record_path: record_paths.relative,
				record: discovery.record,
			});
			releases.sort_by(|left, right| {
				left.record
					.created_at
					.cmp(&right.record.created_at)
					.then_with(|| left.tag_name.cmp(&right.tag_name))
			});
		}
	}

	if releases.is_empty() {
		return Ok(SyncReleaseReport {
			mode,
			status: "up_to_date".to_string(),
			dry_run,
			cut_id: None,
			synced_tags: Vec::new(),
			synced_commits: Vec::new(),
			deleted_changesets: Vec::new(),
			updated_changelogs: Vec::new(),
			synced_record_paths: Vec::new(),
			updated_manifests: Vec::new(),
			changed_files: Vec::new(),
		});
	}

	// Plan and validate every record-derived mutation before touching disk, so a
	// bad path in a later record cannot leave earlier changesets or changelogs
	// half-applied.
	let mut deleted_changesets = Vec::new();
	let mut updated_changelogs = Vec::new();
	let mut synced_record_paths = Vec::new();
	let mut updated_manifests = Vec::new();
	let mut changed_files_set = BTreeSet::new();

	let mut deletion_targets = Vec::new();
	let mut seen_deletions = BTreeSet::new();
	for published in &releases {
		for path in &published.record.deleted_changesets {
			let relative = repository_relative_path(root, path, "remove a synced changeset")?;

			if !seen_deletions.insert(relative.clone()) {
				continue;
			}

			let absolute = root.join(&relative);

			if absolute.is_file() {
				deletion_targets.push((relative, absolute));
			}
		}
	}

	// Replay rendered changelog sections in chronological order (oldest -> newest).
	let mut changelog_buffers = BTreeMap::<PathBuf, (String, bool)>::new();
	for published in &releases {
		for entry in &published.record.changelogs {
			if entry.rendered.trim().is_empty() {
				continue;
			}

			let relative = repository_relative_path(root, &entry.path, "sync a changelog")?;
			let absolute = root.join(&relative);
			let version = record_version_for_owner(&published.record, &entry.owner_id);
			let (current, dirty) = match changelog_buffers.remove(&relative) {
				Some(state) => state,
				None => (read_file_or_empty(&absolute)?, false),
			};

			if version
				.as_deref()
				.is_some_and(|version| changelog_contains_version(&current, version))
			{
				changelog_buffers.insert(relative, (current, dirty));
				continue;
			}

			let next = insert_changelog_section(&current, &entry.rendered, None);
			changelog_buffers.insert(relative, (next, true));
		}
	}

	let changelog_writes = changelog_buffers
		.into_iter()
		.filter(|(_, (_, dirty))| *dirty)
		.collect::<BTreeMap<_, _>>();

	let mut synced_record_writes = Vec::with_capacity(releases.len());
	for published in &releases {
		let relative = repository_relative_path(
			root,
			&published.record_path,
			"write a synced release record",
		)?;
		let mut synced_record = published.record.clone();
		synced_record.synced_from_commit = Some(published.commit.clone());
		let serialized = serde_json::to_string_pretty(&synced_record).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to serialize synced release record: {error}"
			))
		})?;
		synced_record_writes.push((relative, serialized));
	}

	let mut manifest_writes = Vec::new();
	if matches!(mode, SyncMode::SemanticPr | SyncMode::MergePr) {
		for update in plan_synced_manifest_versions(root, configuration, &releases)? {
			let relative = repository_relative_path(root, &update.path, "sync a manifest")?;
			manifest_writes.push((relative, update));
		}
	}

	// Every path is now validated. Apply the changesets, changelogs, and
	// manifests, then write the durable synced record last so a failure during
	// the manifest updates still retries on the next sync run.
	for (relative, absolute) in deletion_targets {
		if !dry_run {
			fs::remove_file(&absolute).map_err(|error| {
				MonochangeError::Io(format!(
					"failed to remove synced changeset {}: {error}",
					absolute.display()
				))
			})?;
		}

		deleted_changesets.push(relative.clone());
		changed_files_set.insert(relative);
	}

	for (relative, (content, _)) in changelog_writes {
		if !dry_run {
			let absolute = root.join(&relative);

			if let Some(parent) = absolute.parent() {
				fs::create_dir_all(parent).map_err(|error| {
					MonochangeError::Io(format!(
						"failed to create changelog directory {}: {error}",
						parent.display()
					))
				})?;
			}

			fs::write(&absolute, content).map_err(|error| {
				MonochangeError::Io(format!(
					"failed to write synced changelog {}: {error}",
					absolute.display()
				))
			})?;
		}

		updated_changelogs.push(relative.clone());
		changed_files_set.insert(relative);
	}

	for (relative, update) in manifest_writes {
		if !dry_run {
			if let Some(parent) = update.path.parent() {
				fs::create_dir_all(parent).map_err(|error| {
					MonochangeError::Io(format!(
						"failed to create parent directory {}: {error}",
						parent.display()
					))
				})?;
			}

			fs::write(&update.path, &update.content).map_err(|error| {
				MonochangeError::Io(format!(
					"failed to write synced manifest {}: {error}",
					update.path.display()
				))
			})?;
		}

		updated_manifests.push(relative.clone());
		changed_files_set.insert(relative);
	}

	// The durable `synced_from_commit` marker is written only after manifest
	// operations succeed, so an interrupted semantic sync re-collects the same
	// tag and can finish the remaining manifest work on retry.
	for (relative, serialized) in synced_record_writes {
		if !dry_run {
			let absolute = root.join(&relative);

			if let Some(parent) = absolute.parent() {
				fs::create_dir_all(parent).map_err(|error| {
					MonochangeError::Io(format!(
						"failed to create release record dir {}: {error}",
						parent.display()
					))
				})?;
			}

			fs::write(&absolute, format!("{serialized}\n")).map_err(|error| {
				MonochangeError::Io(format!(
					"failed to write synced release record {}: {error}",
					absolute.display()
				))
			})?;
		}

		synced_record_paths.push(relative.clone());
		changed_files_set.insert(relative);
	}

	let synced_tags = releases
		.iter()
		.map(|published| published.tag_name.clone())
		.collect::<Vec<_>>();
	let synced_commits = releases
		.iter()
		.map(|published| published.commit.clone())
		.collect::<Vec<_>>();
	let cut_id = synced_tags.last().cloned();

	Ok(SyncReleaseReport {
		mode,
		status: if dry_run {
			"dry_run".to_string()
		} else {
			"completed".to_string()
		},
		dry_run,
		cut_id,
		synced_tags,
		synced_commits,
		deleted_changesets,
		updated_changelogs,
		synced_record_paths,
		updated_manifests,
		changed_files: changed_files_set.into_iter().collect(),
	})
}

/// Plan manifest and configured versioned-file updates for any packages whose
/// released version in `releases` is newer than the current manifest version on
/// disk. Callers validate every returned path before applying any update.
fn plan_synced_manifest_versions(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	releases: &[PublishedReleaseRecord],
) -> MonochangeResult<Vec<FileUpdate>> {
	// Resolve highest released version per target id / package id across coalesced releases.
	let mut target_versions = BTreeMap::<String, semver::Version>::new();
	for published in releases {
		for target in &published.record.release_targets {
			let Ok(version) = semver::Version::parse(&target.version) else {
				continue;
			};

			insert_higher_version(&mut target_versions, &target.id, &version);

			for member in &target.members {
				insert_higher_version(&mut target_versions, member, &version);
			}
		}

		for (id, version_str) in &published.record.versions {
			if let Ok(version) = semver::Version::parse(version_str) {
				insert_higher_version(&mut target_versions, id, &version);
			}
		}
	}

	if target_versions.is_empty() {
		return Ok(Vec::new());
	}

	let discovery = discover_release_workspace(root, configuration)?;
	let mut decisions = Vec::new();

	for package in &discovery.packages {
		let config_id = package
			.metadata
			.get("config_id")
			.cloned()
			.unwrap_or_else(|| package.id.clone());
		let Some(released_version) = target_versions
			.get(&config_id)
			.or_else(|| target_versions.get(&package.name))
			.or_else(|| target_versions.get(&package.id))
		else {
			continue;
		};

		// Skip packages opting into tag-only versions (e.g. versionless `0.0.0-dev`
		// packages mixed into a polyglot repo).
		if configuration
			.effective_release_identity(&config_id)
			.is_some_and(|identity| identity.version_source.reads_from_tags())
		{
			continue;
		}

		let needs_update = package
			.current_version
			.as_ref()
			.is_none_or(|current| released_version > current);

		if !needs_update {
			continue;
		}

		let group_id = configuration
			.groups
			.iter()
			.find(|group| group.packages.iter().any(|member| member == &config_id))
			.map(|group| group.id.clone());

		decisions.push(ReleaseDecision {
			package_id: package.id.clone(),
			trigger_type: "sync".to_string(),
			recommended_bump: BumpSeverity::Patch,
			planned_version: Some(released_version.clone()),
			group_id,
			reasons: vec![format!("sync published version {released_version}")],
			upstream_sources: Vec::new(),
			warnings: Vec::new(),
		});
	}

	if decisions.is_empty() {
		return Ok(Vec::new());
	}

	let plan = ReleasePlan {
		workspace_root: root.to_path_buf(),
		decisions,
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};

	let manifest_updates = build_manifest_updates_parallel(&discovery.packages, &plan)?;
	let dependency_sync_updates =
		build_dependency_sync_updates(root, &discovery, &plan, &manifest_updates)?;
	let manifest_updates = merge_file_updates(manifest_updates, dependency_sync_updates);
	let versioned_file_updates = build_versioned_file_updates_with_base_updates(
		root,
		configuration,
		&discovery.packages,
		&plan,
		&manifest_updates,
		&ResolvedReleaseValues::default(),
	)?;
	let all_updates = merge_file_updates(manifest_updates, versioned_file_updates);

	Ok(all_updates)
}

fn insert_higher_version(
	map: &mut BTreeMap<String, semver::Version>,
	key: &str,
	candidate: &semver::Version,
) {
	match map.get(key) {
		Some(existing) if existing >= candidate => {}
		_ => {
			map.insert(key.to_string(), candidate.clone());
		}
	}
}

/// Build the [`SourceChangeRequest`] for `OpenSyncRequest`.
#[must_use]
pub(crate) fn build_sync_change_request(
	source: &SourceConfiguration,
	report: &SyncReleaseReport,
) -> SourceChangeRequest {
	let sync = &source.pull_requests.sync;
	let cut_id = report.cut_id.as_deref().unwrap_or("release");
	let title = render_cut_or_sync_title(&sync.title, cut_id, &source.pull_requests.base, None);
	let mut body_lines = vec![
		format!(
			"Synchronizes published release bookkeeping for {} onto `{}`.",
			report
				.synced_tags
				.iter()
				.map(|tag| format!("`{tag}`"))
				.collect::<Vec<_>>()
				.join(", "),
			source.pull_requests.base
		),
		String::new(),
		format!("- Sync mode: `{}`", sync_mode_label(report.mode)),
		format!("- Deleted changesets: {}", report.deleted_changesets.len()),
		format!("- Updated changelogs: {}", report.updated_changelogs.len()),
		format!(
			"- Synced release records: {}",
			report.synced_record_paths.len()
		),
		format!("- Updated manifests: {}", report.updated_manifests.len()),
	];

	if !report.synced_commits.is_empty() {
		body_lines.push(format!(
			"- Tagged release commits: {}",
			report
				.synced_commits
				.iter()
				.map(|commit| format!("`{}`", crate::short_commit_sha(commit)))
				.collect::<Vec<_>>()
				.join(", ")
		));
	}

	let body = body_lines.join("\n");

	SourceChangeRequest {
		provider: source.provider,
		repository: format!("{}/{}", source.owner, source.repo),
		owner: source.owner.clone(),
		repo: source.repo.clone(),
		base_branch: source.pull_requests.base.clone(),
		head_branch: sync.branch.clone(),
		title: title.clone(),
		body: body.clone(),
		labels: sync.labels.clone(),
		auto_merge: sync.auto_merge,
		commit_message: CommitMessage {
			subject: title,
			body: Some(body),
		},
		body_truncation: None,
	}
}

/// Commit synced bookkeeping locally, creating a multi-parent bridge commit
/// `(HEAD, C_rel_1, ...)` when `report.mode == SyncMode::MergePr`.
pub(crate) async fn commit_sync_release_local(
	root: &Path,
	report: &SyncReleaseReport,
	message: &CommitMessage,
	no_verify: bool,
	stage_all: bool,
) -> MonochangeResult<String> {
	for path in &report.changed_files {
		repository_relative_path(root, path, "stage a synced release path")?;
	}

	if stage_all {
		git_stage_all(root).await?;
	} else {
		git_stage_paths(root, &report.changed_files).await?;
	}

	if report.mode == SyncMode::MergePr && !report.synced_commits.is_empty() {
		let head = git_head_commit(root).await?;
		let mut parents = Vec::with_capacity(1 + report.synced_commits.len());
		parents.push(head.as_str());

		for commit in &report.synced_commits {
			parents.push(commit.as_str());
		}

		return git_create_bridge_commit(root, &parents, message).await;
	}

	git_commit_paths(root, message, no_verify).await?;
	git_head_commit(root).await
}

/// Render a [`SyncReleaseReport`] in text, markdown, or JSON format.
pub(crate) fn render_sync_release_report(
	report: &SyncReleaseReport,
	format: OutputFormat,
) -> MonochangeResult<String> {
	match format {
		OutputFormat::Json | OutputFormat::JsonMin => {
			format.render_json_value(report, "sync release report")
		}
		OutputFormat::Text | OutputFormat::Markdown => {
			let mut lines = vec![format!(
				"Sync release ({}, mode: {}): {}",
				report.status,
				sync_mode_label(report.mode),
				if report.synced_tags.is_empty() {
					"none".to_string()
				} else {
					report.synced_tags.join(", ")
				}
			)];
			lines.push(format!(
				"- deleted changesets: {}",
				report.deleted_changesets.len()
			));
			lines.push(format!(
				"- updated changelogs: {}",
				report.updated_changelogs.len()
			));
			lines.push(format!(
				"- synced release records: {}",
				report.synced_record_paths.len()
			));
			lines.push(format!(
				"- updated manifests: {}",
				report.updated_manifests.len()
			));
			Ok(lines.join("\n"))
		}
	}
}

fn sync_mode_label(mode: SyncMode) -> &'static str {
	match mode {
		SyncMode::BookkeepingOnly => "bookkeeping_only",
		SyncMode::MergePr => "merge_pr",
		SyncMode::None => "none",
		_ => "semantic_pr",
	}
}

fn record_version_for_owner(record: &ReleaseRecord, owner_id: &str) -> Option<String> {
	record
		.release_targets
		.iter()
		.find(|target| target.id == owner_id)
		.map(|target| target.version.clone())
		.or_else(|| record.versions.get(owner_id).cloned())
		.or_else(|| record.version.clone())
}

/// Resolve a record-derived path to a repository-relative path.
///
/// Release records written by older monochange versions may store absolute
/// paths under the workspace root. Both layouts are honored; anything outside
/// the repository, including paths that escape through `..` or a symbolic
/// link, is rejected before callers read, remove, write, or stage it.
fn repository_relative_path(root: &Path, path: &Path, action: &str) -> MonochangeResult<PathBuf> {
	if path.as_os_str().is_empty() {
		return Err(outside_repository_error(root, path, action));
	}

	let relative = if path.is_absolute() {
		if let Ok(relative) = path.strip_prefix(root) {
			relative.to_path_buf()
		} else if let Some(relative) = relative_to_root(root, path) {
			relative
		} else {
			return Err(outside_repository_error(root, path, action));
		}
	} else {
		path.to_path_buf()
	};

	let has_only_normal_components = relative
		.components()
		.all(|component| matches!(component, Component::Normal(_) | Component::CurDir));

	if relative.as_os_str().is_empty() || !has_only_normal_components {
		return Err(outside_repository_error(root, path, action));
	}

	ensure_no_symlink_escapes_root(root, &relative, path, action)?;
	Ok(relative)
}

fn outside_repository_error(root: &Path, path: &Path, action: &str) -> MonochangeError {
	MonochangeError::Config(format!(
		"refusing to {action} `{}` because it is outside repository root `{}`",
		path.display(),
		root.display()
	))
}

/// Reject a path when any existing component below `root` is a symbolic link
/// that resolves outside the canonical repository root.
fn ensure_no_symlink_escapes_root(
	root: &Path,
	relative: &Path,
	original: &Path,
	action: &str,
) -> MonochangeResult<()> {
	let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
	let mut current = root.to_path_buf();

	for component in relative.components() {
		let Component::Normal(name) = component else {
			continue;
		};

		current.push(name);

		match fs::symlink_metadata(&current) {
			Ok(metadata) if metadata.file_type().is_symlink() => {
				let resolved = fs::canonicalize(&current).map_err(|error| {
					MonochangeError::Config(format!(
						"refusing to {action} `{}` because symbolic link `{}` does not resolve inside repository root `{}`: {error}",
						original.display(),
						current.display(),
						root.display()
					))
				})?;

				if !resolved.starts_with(&canonical_root) {
					return Err(MonochangeError::Config(format!(
						"refusing to {action} `{}` because symbolic link `{}` resolves outside repository root `{}`",
						original.display(),
						current.display(),
						root.display()
					)));
				}

				current = resolved;
			}
			Ok(_) => {}
			Err(error)
				if matches!(
					error.kind(),
					std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
				) =>
			{
				break;
			}
			Err(error) => {
				return Err(MonochangeError::Io(format!(
					"failed to inspect repository path {}: {error}",
					current.display()
				)));
			}
		}
	}

	Ok(())
}

fn read_file_or_empty(path: &Path) -> MonochangeResult<String> {
	if !path.is_file() {
		return Ok(String::new());
	}

	fs::read_to_string(path)
		.map_err(|error| MonochangeError::Io(format!("failed to read {}: {error}", path.display())))
}

#[cfg(test)]
#[path = "__tests__/release_cut_tests.rs"]
mod tests;
