use std::cell::Cell;
use std::io::BufRead;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::IsTerminal;

use similar::TextDiff;

use super::*;
use crate::cli_runtime::build_release_request_result;
use crate::cli_runtime::string_step_input;
use crate::git_support::git_stage_all;
use crate::output::text::Outcome;
use crate::output::text::TableCell;
use crate::output::text::TextReport;
use crate::output::text::TextTheme;
use crate::output::text::Tone;
use crate::output::text::plural;

thread_local! {
	pub(crate) static FORCE_BUILD_FILE_DIFF_PREVIEWS_ERROR: Cell<bool> = const { Cell::new(false) };
}

thread_local! {
	pub(crate) static DEDUPLICATED_CACHE: std::cell::RefCell<std::collections::HashSet<(PathBuf, String)>> =
		std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Path to the persistent deduplication index relative to the workspace root.
const DEDUP_INDEX_PATH: &str = ".monochange/local/release-index.jsonl";

/// Load the persistent deduplication index as a set of hashes.
///
/// The index is stored as a JSONL file at `.monochange/local/release-index.jsonl`.
/// Each line is a JSON object with a `hash` field. Missing or unreadable files
/// are treated as empty indices.
fn load_dedup_index(root: &Path) -> std::collections::HashSet<String> {
	let path = root.join(DEDUP_INDEX_PATH);
	let Ok(file) = fs::File::open(&path) else {
		return std::collections::HashSet::new();
	};
	let reader = BufReader::new(file);
	load_dedup_index_from_reader(reader).unwrap_or_default()
}

fn load_dedup_index_from_reader(reader: impl BufRead) -> Option<std::collections::HashSet<String>> {
	let mut index = std::collections::HashSet::new();
	for line in reader.lines() {
		let Ok(line) = line else {
			return None;
		};
		let line = line.trim();
		if line.is_empty() {
			continue;
		}
		if let Some(hash) = parse_dedup_index_hash(line) {
			index.insert(hash.to_owned());
		}
	}
	Some(index)
}

fn parse_dedup_index_hash(line: &str) -> Option<&str> {
	#[derive(serde::Deserialize)]
	struct DedupIndexEntry<'a> {
		#[serde(borrow)]
		hash: &'a str,
	}

	serde_json::from_str::<DedupIndexEntry<'_>>(line)
		.ok()
		.map(|entry| entry.hash)
}

/// Save the persistent deduplication index atomically.
///
/// Writes to a temporary file next to the target and renames it into place.
/// This avoids corrupting the index if the process is interrupted mid-write.
fn save_dedup_index(
	root: &Path,
	index: &std::collections::HashSet<String>,
) -> MonochangeResult<()> {
	let path = root.join(DEDUP_INDEX_PATH);
	let parent = path.parent().unwrap_or(root);
	fs::create_dir_all(parent)
		.map_err(|error| MonochangeError::Io(format!("create dedup index dir: {error}")))?;
	let mut hashes = index.iter().map(String::as_str).collect::<Vec<_>>();
	hashes.sort_unstable();
	let temp = path.with_extension("tmp");
	let file = fs::File::create(&temp)
		.map_err(|error| MonochangeError::Io(format!("write dedup index: {error}")))?;
	let mut writer = BufWriter::new(file);
	for (position, hash) in hashes.iter().enumerate() {
		if position > 0 {
			std::io::Write::write_all(&mut writer, b"\n")
				.map_err(|error| MonochangeError::Io(format!("write dedup index: {error}")))?;
		}
		std::io::Write::write_fmt(&mut writer, format_args!(r#"{{"hash":"{hash}"}}"#))
			.map_err(|error| MonochangeError::Io(format!("write dedup index: {error}")))?;
	}
	std::io::Write::flush(&mut writer)
		.map_err(|error| MonochangeError::Io(format!("write dedup index: {error}")))?;
	fs::rename(&temp, &path)
		.map_err(|error| MonochangeError::Io(format!("rename dedup index: {error}")))?;
	Ok(())
}

/// Add a hash to the persistent deduplication index.
fn add_to_dedup_index(root: &Path, hash: &str) -> MonochangeResult<()> {
	let mut index = load_dedup_index(root);
	index.insert(hash.to_string());
	save_dedup_index(root, &index)
}

/// Remove a hash from the persistent deduplication index.
fn remove_from_dedup_index(root: &Path, hash: &str) -> MonochangeResult<()> {
	let mut index = load_dedup_index(root);
	index.remove(hash);
	save_dedup_index(root, &index)
}

pub(crate) async fn build_release_targets(
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
	changeset_paths: &[PathBuf],
) -> Vec<ReleaseTarget> {
	let changes_count = changeset_paths.len();
	let package_by_id = packages
		.iter()
		.map(|package| (package.id.as_str(), package))
		.collect::<BTreeMap<_, _>>();
	let source = configuration.source.as_ref();
	let defaults_release_title = configuration.defaults.release_title.as_deref();
	let defaults_changelog_title = configuration.defaults.changelog_version_title.as_deref();

	// Cache the sorted tag list once for the whole command.
	//
	// Performance note:
	// the previous implementation ran `git tag --list --sort=-v:refname` once per
	// release target. On a repository with multiple release identities that turned
	// a tiny formatting helper into repeated subprocess latency. The target builder
	// only needs a stable view of tags for the current command, so sharing one
	// loaded list avoids re-running the same git command over and over.
	let sorted_tags = load_sorted_tags(&configuration.root_path).await;
	let configured_package_by_id = configuration
		.packages
		.iter()
		.map(|package| (package.id.as_str(), package))
		.collect::<std::collections::HashMap<_, _>>();
	let mut group_by_package_id = std::collections::HashMap::with_capacity(
		configuration
			.groups
			.iter()
			.map(|group| group.packages.len())
			.sum(),
	);
	for group in &configuration.groups {
		for package_id in &group.packages {
			group_by_package_id
				.entry(package_id.as_str())
				.or_insert(group);
		}
	}
	let planned_group_by_id = plan
		.groups
		.iter()
		.filter(|group| group.recommended_bump.is_release())
		.map(|group| (group.group_id.as_str(), group))
		.collect::<std::collections::HashMap<_, _>>();

	let mut release_targets = Vec::with_capacity(configuration.groups.len() + plan.decisions.len());
	release_targets.extend(configuration.groups.iter().filter_map(|group| {
		planned_group_by_id.get(group.id.as_str()).and_then(|pg| {
			pg.planned_version.as_ref().map(|version| {
				let vs = version.to_string();
				let tag = render_tag_name(&group.id, &vs, "group", &group.version_format);
				let prev = find_previous_tag_in(
					&tag,
					&sorted_tags,
					&group.id,
					"group",
					&group.version_format,
				);
				let ctx = TitleRenderContext::new(
					&group.id,
					&vs,
					changes_count,
					source,
					&tag,
					prev.as_ref().map(|(tag, version)| (*tag, version)),
				);
				let rt = effective_title_template(
					group.release_title.as_deref(),
					defaults_release_title,
					default_release_title_for_format(&group.version_format),
				);
				let ct = effective_title_template(
					group.changelog_version_title.as_deref(),
					defaults_changelog_title,
					default_changelog_version_title_for_format(&group.version_format),
				);
				ReleaseTarget {
					id: group.id.clone(),
					kind: ReleaseOwnerKind::Group,
					version: vs,
					tag: group.tag,
					release: group.release,
					version_format: group.version_format.clone(),
					tag_name: tag,
					members: group.packages.clone(),
					rendered_title: ctx.render(rt),
					rendered_changelog_title: ctx.render(ct),
					floating_tags: group.floating_tags.clone(),
				}
			})
		})
	}));
	for decision in plan
		.decisions
		.iter()
		.filter(|d| d.recommended_bump.is_release() && d.group_id.is_none())
	{
		let Some(package) = package_by_id.get(decision.package_id.as_str()).copied() else {
			continue;
		};
		let Some(version) = decision.planned_version.as_ref() else {
			continue;
		};
		let config_id = package
			.metadata
			.get("config_id")
			.cloned()
			.unwrap_or_else(|| package.name.clone());
		let Some(package_definition) = configured_package_by_id.get(config_id.as_str()).copied()
		else {
			continue;
		};
		let (
			owner_id,
			owner_kind,
			tag_enabled,
			release_enabled,
			version_format,
			members,
			floating_tags_for_release_target,
		) = if let Some(group) = group_by_package_id.get(config_id.as_str()).copied() {
			(
				&group.id,
				ReleaseOwnerKind::Group,
				group.tag,
				group.release,
				group.version_format.clone(),
				group.packages.clone(),
				group.floating_tags.clone(),
			)
		} else {
			(
				&package_definition.id,
				ReleaseOwnerKind::Package,
				package_definition.tag,
				package_definition.release,
				package_definition.version_format.clone(),
				vec![package_definition.id.clone()],
				package_definition.floating_tags.clone(),
			)
		};
		let vs = version.to_string();
		let tag = render_tag_name(
			owner_id,
			&vs,
			package_definition.package_type.as_str(),
			&version_format,
		);
		let prev = find_previous_tag_in(
			&tag,
			&sorted_tags,
			owner_id,
			package_definition.package_type.as_str(),
			&version_format,
		);
		let ctx = TitleRenderContext::new(
			owner_id,
			&vs,
			changes_count,
			source,
			&tag,
			prev.as_ref().map(|(tag, version)| (*tag, version)),
		);
		let rt = effective_title_template(
			package_definition.release_title.as_deref(),
			defaults_release_title,
			default_release_title_for_format(&version_format),
		);
		let ct = effective_title_template(
			package_definition.changelog_version_title.as_deref(),
			defaults_changelog_title,
			default_changelog_version_title_for_format(&version_format),
		);
		release_targets.push(ReleaseTarget {
			id: owner_id.clone(),
			kind: owner_kind,
			version: vs,
			tag: tag_enabled,
			release: release_enabled,
			version_format,
			tag_name: tag,
			members,
			rendered_title: ctx.render(rt),
			rendered_changelog_title: ctx.render(ct),
			floating_tags: floating_tags_for_release_target,
		});
	}
	release_targets.sort_by(|left, right| left.id.cmp(&right.id));
	release_targets
}

pub(crate) fn build_package_publication_targets(
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> Vec<PackagePublicationTarget> {
	let package_by_id = packages
		.iter()
		.map(|package| (package.id.as_str(), package))
		.collect::<std::collections::HashMap<_, _>>();
	let configured_package_by_id = configuration
		.packages
		.iter()
		.map(|package| (package.id.as_str(), package))
		.collect::<std::collections::HashMap<_, _>>();
	let mut targets = plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
		.filter_map(|decision| {
			let package = package_by_id.get(decision.package_id.as_str()).copied()?;
			let version = decision.planned_version.as_ref()?;
			let config_id = package
				.metadata
				.get("config_id")
				.cloned()
				.unwrap_or_else(|| package.name.clone());
			let package_definition = configured_package_by_id.get(config_id.as_str()).copied()?;
			if !package_definition.publish.enabled
				|| matches!(
					package.publish_state,
					monochange_core::PublishState::Private
						| monochange_core::PublishState::Excluded
				) {
				return None;
			}
			Some(PackagePublicationTarget {
				package: config_id,
				ecosystem: package.ecosystem,
				registry: package_definition.publish.registry.clone(),
				version: version.to_string(),
				mode: package_definition.publish.mode,
				flow: package_definition.publish.flow,
				trusted_publishing: package_definition.publish.trusted_publishing.clone(),
				attestations: package_definition.publish.attestations.clone(),
				timeout: package_definition.publish.timeout.clone(),
				fail_on_duplicate: package_definition.publish.fail_on_duplicate,
			})
		})
		.collect::<Vec<_>>();
	targets.sort_by(|left, right| left.package.cmp(&right.package));
	targets
}

pub(crate) fn build_manifest_updates_parallel(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	#[cfg(all(feature = "cargo", feature = "npm", feature = "deno", feature = "dart"))]
	{
		let ((cargo_updates, npm_updates), (deno_updates, dart_updates)) = rayon::join(
			|| {
				rayon::join(
					|| build_cargo_manifest_updates(packages, plan),
					|| build_npm_manifest_updates(packages, plan),
				)
			},
			|| {
				rayon::join(
					|| build_deno_manifest_updates(packages, plan),
					|| build_dart_manifest_updates(packages, plan),
				)
			},
		);
		Ok([cargo_updates?, npm_updates?, deno_updates?, dart_updates?].concat())
	}

	#[cfg(not(all(feature = "cargo", feature = "npm", feature = "deno", feature = "dart")))]
	{
		let mut updates = Vec::new();
		#[cfg(feature = "cargo")]
		updates.extend(build_cargo_manifest_updates(packages, plan)?);
		#[cfg(feature = "npm")]
		updates.extend(build_npm_manifest_updates(packages, plan)?);
		#[cfg(feature = "deno")]
		updates.extend(build_deno_manifest_updates(packages, plan)?);
		#[cfg(feature = "dart")]
		updates.extend(build_dart_manifest_updates(packages, plan)?);
		Ok(updates)
	}
}

#[allow(clippy::match_same_arms)]
pub(crate) fn render_tag_name(
	id: &str,
	version: &str,
	ecosystem: &str,
	version_format: &VersionFormat,
) -> String {
	version_format
		.render_tag(id, version, ecosystem)
		.unwrap_or_else(|_| format!("v{version}"))
}

/// Dispatch tag URL generation to the configured hosted source adapter.
pub(crate) fn tag_url_for_provider(source: &SourceConfiguration, tag_name: &str) -> String {
	hosted_sources::hosted_source_adapter(source.provider).tag_url(source, tag_name)
}

/// Dispatch compare URL generation to the configured hosted source adapter.
pub(crate) fn compare_url_for_provider(
	source: &SourceConfiguration,
	previous_tag: &str,
	current_tag: &str,
) -> String {
	hosted_sources::hosted_source_adapter(source.provider).compare_url(
		source,
		previous_tag,
		current_tag,
	)
}

pub(crate) async fn load_sorted_tags(root: &Path) -> Vec<String> {
	let output = match monochange_core::git::git_command_output(
		root,
		&["tag", "--list", "--sort=-v:refname"],
	)
	.await
	{
		Ok(output) if output.status.success() => output,
		_ => return Vec::new(),
	};
	parse_sorted_tag_lines(&output.stdout)
}

/// Parse `git tag --list` output into trimmed, non-empty tag names.
pub(crate) fn parse_sorted_tag_lines(stdout: &[u8]) -> Vec<String> {
	String::from_utf8_lossy(stdout)
		.lines()
		.map(str::trim)
		.filter(|tag| !tag.is_empty())
		.map(ToString::to_string)
		.collect()
}

/// Resolve static owner fields while retaining one spelling for version variables.
pub(crate) fn resolved_release_tag_template(
	owner_id: &str,
	ecosystem: &str,
	version_format: &VersionFormat,
) -> String {
	version_format
		.as_template()
		.replace("{{ name }}", owner_id)
		.replace("{{name}}", owner_id)
		.replace("{{ ecosystem }}", ecosystem)
		.replace("{{ecosystem}}", ecosystem)
		.replace("{{version}}", "{{ version }}")
}

/// Read a `SemVer` only when rendering it reproduces the complete owner's tag.
///
/// Round-trip matching preserves literal suffixes and repeated version variables,
/// and avoids guessing a separator from letters inside a prerelease identifier.
pub(crate) fn matching_release_tag_version(
	tag: &str,
	owner_id: &str,
	ecosystem: &str,
	version_format: &VersionFormat,
) -> Option<semver::Version> {
	let template = resolved_release_tag_template(owner_id, ecosystem, version_format);
	let (prefix, _) = template.split_once("{{ version }}")?;
	let remaining = tag.strip_prefix(prefix)?;
	remaining
		.char_indices()
		.map(|(end, _)| end)
		.chain(std::iter::once(remaining.len()))
		.filter_map(|end| {
			remaining
				.get(..end)
				.and_then(|candidate| semver::Version::parse(candidate).ok())
		})
		.find(|version| {
			version_format
				.render_tag(owner_id, &version.to_string(), ecosystem)
				.is_ok_and(|rendered| rendered == tag)
		})
}

/// Select the highest matching `SemVer` independently of Git's tag sort order.
pub(crate) fn latest_release_tag_in<'a>(
	tags: &'a [String],
	owner_id: &str,
	ecosystem: &str,
	version_format: &VersionFormat,
) -> Option<(&'a str, semver::Version)> {
	tags.iter()
		.filter_map(|tag| {
			matching_release_tag_version(tag, owner_id, ecosystem, version_format)
				.map(|version| (tag.as_str(), version))
		})
		.max_by(|left, right| left.1.cmp(&right.1))
}

/// Select the highest matching release strictly before the current `SemVer`.
pub(crate) fn find_previous_tag_in<'a>(
	current_tag: &str,
	tags: &'a [String],
	owner_id: &str,
	ecosystem: &str,
	version_format: &VersionFormat,
) -> Option<(&'a str, semver::Version)> {
	let current_version =
		matching_release_tag_version(current_tag, owner_id, ecosystem, version_format)?;
	tags.iter()
		.filter_map(|tag| {
			matching_release_tag_version(tag, owner_id, ecosystem, version_format)
				.filter(|version| version < &current_version)
				.map(|version| (tag.as_str(), version))
		})
		.max_by(|left, right| left.1.cmp(&right.1))
}

struct TitleRenderContext {
	id: String,
	version: String,
	previous_version: String,
	date: String,
	time: String,
	datetime: String,
	changes_count: usize,
	tag_url: String,
	compare_url: String,
}

impl TitleRenderContext {
	fn new(
		id: &str,
		version: &str,
		changes_count: usize,
		source: Option<&SourceConfiguration>,
		tag_name: &str,
		previous_tag: Option<(&str, &semver::Version)>,
	) -> Self {
		Self::with_datetime(
			id,
			version,
			changes_count,
			source,
			tag_name,
			previous_tag,
			resolve_release_datetime(),
		)
	}

	fn with_datetime(
		id: &str,
		version: &str,
		changes_count: usize,
		source: Option<&SourceConfiguration>,
		tag_name: &str,
		previous_tag: Option<(&str, &semver::Version)>,
		now: chrono::NaiveDateTime,
	) -> Self {
		let date = now.format("%Y-%m-%d").to_string();
		let time = now.format("%H:%M:%S").to_string();
		let datetime = now.format("%Y-%m-%dT%H:%M:%S").to_string();
		let tag_url = source
			.map(|s| tag_url_for_provider(s, tag_name))
			.unwrap_or_default();
		let compare_url = match (source, previous_tag) {
			(Some(s), Some((prev, _))) => compare_url_for_provider(s, prev, tag_name),
			_ => tag_url.clone(),
		};
		let previous_version = previous_tag
			.map(|(_, version)| version.to_string())
			.unwrap_or_default();
		Self {
			id: id.to_string(),
			version: version.to_string(),
			previous_version,
			date,
			time,
			datetime,
			changes_count,
			tag_url,
			compare_url,
		}
	}

	fn render(&self, template: &str) -> String {
		let context = minijinja::context! {
			id => &self.id,
			version => &self.version,
			previous_version => &self.previous_version,
			date => &self.date,
			time => &self.time,
			datetime => &self.datetime,
			changes_count => self.changes_count,
			tag_url => &self.tag_url,
			compare_url => &self.compare_url,
		};
		let jinja_value = minijinja::Value::from_serialize(&context);
		render_jinja_template(template, &jinja_value).unwrap_or_else(|_| self.version.clone())
	}
}

pub(crate) fn resolve_release_datetime() -> chrono::NaiveDateTime {
	use chrono::NaiveDate;
	use chrono::NaiveDateTime;

	let Ok(env_date) = std::env::var("MONOCHANGE_RELEASE_DATE") else {
		return chrono::Local::now().naive_local();
	};

	if let Ok(ndt) = NaiveDateTime::parse_from_str(&env_date, "%Y-%m-%dT%H:%M:%S") {
		return ndt;
	}

	if let Ok(nd) = NaiveDate::parse_from_str(&env_date, "%Y-%m-%d") {
		return nd.and_hms_opt(0, 0, 0).unwrap_or_default();
	}

	chrono::Local::now().naive_local()
}

pub(crate) fn effective_title_template<'a>(
	specific: Option<&'a str>,
	defaults: Option<&'a str>,
	builtin: &'a str,
) -> &'a str {
	specific.or(defaults).unwrap_or(builtin)
}

#[allow(clippy::match_same_arms)]
pub(crate) fn default_release_title_for_format(version_format: &VersionFormat) -> &'static str {
	match version_format {
		VersionFormat::Primary => DEFAULT_RELEASE_TITLE_PRIMARY,
		VersionFormat::Namespaced => DEFAULT_RELEASE_TITLE_NAMESPACED,
		_ => DEFAULT_RELEASE_TITLE_PRIMARY,
	}
}

#[allow(clippy::match_same_arms)]
pub(crate) fn default_changelog_version_title_for_format(
	version_format: &VersionFormat,
) -> &'static str {
	match version_format {
		VersionFormat::Primary => DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY,
		VersionFormat::Namespaced => DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED,
		_ => DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY,
	}
}
#[cfg(feature = "cargo")]
pub(crate) fn build_cargo_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	monochange_cargo::build_manifest_updates(packages, plan).map(|updates| {
		updates
			.into_iter()
			.map(FileUpdate::from_manifest_update)
			.collect()
	})
}

#[cfg(feature = "npm")]
pub(crate) fn build_npm_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	monochange_npm::build_manifest_updates(packages, plan).map(|updates| {
		updates
			.into_iter()
			.map(FileUpdate::from_manifest_update)
			.collect()
	})
}

#[cfg(feature = "deno")]
pub(crate) fn build_deno_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	monochange_deno::build_manifest_updates(packages, plan).map(|updates| {
		updates
			.into_iter()
			.map(FileUpdate::from_manifest_update)
			.collect()
	})
}

#[cfg(feature = "dart")]
pub(crate) fn build_dart_manifest_updates(
	packages: &[PackageRecord],
	plan: &ReleasePlan,
) -> MonochangeResult<Vec<FileUpdate>> {
	monochange_dart::build_manifest_updates(packages, plan).map(|updates| {
		updates
			.into_iter()
			.map(FileUpdate::from_manifest_update)
			.collect()
	})
}

#[must_use = "the file update result must be checked"]
pub(crate) fn apply_file_updates(updates: &[FileUpdate]) -> MonochangeResult<()> {
	for update in updates {
		if let Some(parent) = update.path.parent() {
			fs::create_dir_all(parent).map_err(|error| {
				MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
			})?;
		}
		atomic_write(&update.path, &update.content)?;
	}
	Ok(())
}

/// Write file content atomically: write to a temporary file in the same
/// directory, then rename into place. On Unix the rename is atomic within the
/// same filesystem, so the file is either fully written or untouched.
fn atomic_write(path: &Path, content: &[u8]) -> MonochangeResult<()> {
	let parent = path.parent().unwrap_or(path);
	// Capture original permissions before overwriting (if the file exists).
	let original_permissions = fs::metadata(path).ok().map(|meta| meta.permissions());
	let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create temp file in {}: {error}",
			parent.display()
		))
	})?;
	write_temp_file(&mut temp, path, content)?;
	persist_temp_file(temp, path)?;
	// Restore original permissions after rename.
	if let Some(permissions) = original_permissions {
		fs::set_permissions(path, permissions).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to restore permissions on {}: {error}",
				path.display()
			))
		})?;
	}
	Ok(())
}

fn write_temp_file(
	writer: &mut impl std::io::Write,
	path: &Path,
	content: &[u8],
) -> MonochangeResult<()> {
	std::io::Write::write_all(writer, content).map_err(|error| temp_file_write_error(path, &error))
}

fn persist_temp_file(temp: tempfile::NamedTempFile, path: &Path) -> MonochangeResult<()> {
	temp.persist(path)
		.map(|_| ())
		.map_err(|error| temp_file_persist_error(path, &error))
}

fn temp_file_write_error(path: &Path, error: &std::io::Error) -> MonochangeError {
	MonochangeError::Io(format!(
		"failed to write temp file for {}: {error}",
		path.display()
	))
}

fn temp_file_persist_error(path: &Path, error: &tempfile::PersistError) -> MonochangeError {
	MonochangeError::Io(format!(
		"failed to rename temp file to {}: {error}",
		path.display()
	))
}

#[tracing::instrument(skip_all)]
pub(crate) fn build_file_diff_previews(
	root: &Path,
	updates: &[FileUpdate],
) -> MonochangeResult<Vec<PreparedFileDiff>> {
	let colorize_diffs = diff_output_colors_enabled();
	if FORCE_BUILD_FILE_DIFF_PREVIEWS_ERROR.with(Cell::get) {
		return Err(MonochangeError::Io(
			"forced build_file_diff_previews test error".to_string(),
		));
	}
	let mut previews = updates
		.iter()
		.filter_map(|update| {
			let path = root_relative(root, &update.path);
			let before = match fs::read(&update.path) {
				Ok(content) => content,
				Err(error)
					if matches!(
						error.kind(),
						std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
					) =>
				{
					Vec::new()
				}
				Err(error) => {
					return Some(Err(MonochangeError::Io(format!(
						"failed to read {}: {error}",
						update.path.display()
					))));
				}
			};
			(before != update.content).then(|| {
				let diff = render_unified_file_diff(&path, &before, &update.content);
				Ok(PreparedFileDiff {
					path: path.clone(),
					display_diff: render_display_file_diff(&diff, colorize_diffs),
					diff,
				})
			})
		})
		.collect::<MonochangeResult<Vec<_>>>()?;
	previews.sort_by(|left, right| left.path.cmp(&right.path));
	Ok(previews)
}

fn render_unified_file_diff(path: &Path, before: &[u8], after: &[u8]) -> String {
	let before_text = String::from_utf8_lossy(before);
	let after_text = String::from_utf8_lossy(after);
	let context_radius = before_text.lines().count().max(after_text.lines().count());
	let diff = TextDiff::from_lines(before_text.as_ref(), after_text.as_ref());
	let mut unified = diff.unified_diff();
	unified.context_radius(context_radius).header(
		&format!("a/{}", path.display()),
		&format!("b/{}", path.display()),
	);
	unified.to_string().trim_end_matches('\n').to_string()
}

fn render_display_file_diff(diff: &str, colorize: bool) -> String {
	if !colorize {
		return diff.to_string();
	}
	colorize_diff_output(diff)
}

fn diff_output_colors_enabled() -> bool {
	diff_output_supports_color(std::io::stdout().is_terminal())
}

pub(crate) fn diff_output_supports_color(stdout_is_terminal: bool) -> bool {
	if std::env::var_os("NO_COLOR").is_some() {
		return false;
	}
	if std::env::var("CLICOLOR_FORCE")
		.ok()
		.is_some_and(|value| value != "0")
	{
		return true;
	}
	if std::env::var("CLICOLOR")
		.ok()
		.is_some_and(|value| value == "0")
	{
		return false;
	}
	stdout_is_terminal
}

pub(crate) fn colorize_diff_output(diff: &str) -> String {
	let mut output = String::with_capacity(diff.len());
	for (index, line) in diff.lines().enumerate() {
		if index > 0 {
			output.push('\n');
		}
		push_colorized_diff_line(&mut output, line);
	}
	output
}

fn push_colorized_diff_line(output: &mut String, line: &str) {
	if line.starts_with("--- ") || line.starts_with("+++ ") {
		push_ansi_style(output, line, "1;36");
	} else if line.starts_with("@@ ") {
		push_ansi_style(output, line, "36");
	} else if line.starts_with('+') && !line.starts_with("+++") {
		push_ansi_style(output, line, "32");
	} else if line.starts_with('-') && !line.starts_with("---") {
		push_ansi_style(output, line, "31");
	} else if line == r"\ No newline at end of file" {
		push_ansi_style(output, line, "33");
	} else {
		output.push_str(line);
	}
}

fn push_ansi_style(output: &mut String, line: &str, style: &str) {
	output.push_str("\u{1b}[");
	output.push_str(style);
	output.push('m');
	output.push_str(line);
	output.push_str("\u{1b}[0m");
}

pub(crate) fn shared_release_version(plan: &ReleasePlan) -> Option<String> {
	let versions = plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
		.filter_map(|decision| decision.planned_version.as_ref().map(ToString::to_string))
		.collect::<BTreeSet<_>>();
	if versions.len() == 1 {
		versions.first().cloned()
	} else {
		None
	}
}

pub(crate) fn shared_group_version(plan: &ReleasePlan) -> Option<String> {
	let versions = plan
		.groups
		.iter()
		.filter(|group| group.recommended_bump.is_release())
		.filter_map(|group| group.planned_version.as_ref().map(ToString::to_string))
		.collect::<BTreeSet<_>>();
	if versions.len() == 1 {
		versions.first().cloned()
	} else {
		None
	}
}

pub(crate) fn render_discovery_report(
	report: &DiscoveryReport,
	format: OutputFormat,
) -> MonochangeResult<String> {
	match format {
		OutputFormat::Json | OutputFormat::JsonMin => {
			format.render_json_value(&json_discovery_report(report), "discovery report")
		}
		OutputFormat::Markdown | OutputFormat::Text => Ok(text_discovery_report(report)),
	}
}

pub(crate) fn build_release_manifest(
	cli_command: &CliCommandDefinition,
	prepared_release: &PreparedRelease,
	_command_logs: &[String],
) -> ReleaseManifest {
	ReleaseManifest {
		command: cli_command.name.clone(),
		dry_run: prepared_release.dry_run,
		version: prepared_release.version.clone(),
		group_version: prepared_release.group_version.clone(),
		release_targets: prepared_release
			.release_targets
			.iter()
			.map(|target| {
				ReleaseManifestTarget {
					id: target.id.clone(),
					kind: target.kind,
					version: target.version.clone(),
					tag: target.tag,
					release: target.release,
					version_format: target.version_format.clone(),
					tag_name: target.tag_name.clone(),
					members: target.members.clone(),
					rendered_title: target.rendered_title.clone(),
					rendered_changelog_title: target.rendered_changelog_title.clone(),
					floating_tags: target.floating_tags.clone(),
				}
			})
			.collect(),
		released_packages: prepared_release.released_packages.clone(),
		changed_files: prepared_release.changed_files.clone(),
		changelogs: prepared_release
			.changelogs
			.iter()
			.map(|changelog| {
				ReleaseManifestChangelog {
					owner_id: changelog.owner_id.clone(),
					owner_kind: changelog.owner_kind,
					output: changelog.output.clone(),
					stream: changelog.stream.clone(),
					path: changelog.path.clone(),
					format: changelog.format,
					notes: changelog.notes.clone(),
					rendered: changelog.rendered.clone(),
				}
			})
			.collect(),
		package_publications: prepared_release.package_publications.clone(),
		changesets: prepared_release.changesets.clone(),
		deleted_changesets: prepared_release.deleted_changesets.clone(),
		values: prepared_release.versioning.frozen_values(),
		labels: prepared_release.versioning.frozen_labels(),
		// Context is only meaningful when something was rendered from it, and
		// omitting it keeps manifests unchanged for workspaces without values.
		label_inputs: if prepared_release.versioning.is_empty() {
			monochange_core::versioning::LabelInputs::default()
		} else {
			prepared_release.versioning.label_inputs.clone()
		},
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: prepared_release
				.plan
				.decisions
				.iter()
				.map(|decision| {
					ReleaseManifestPlanDecision {
						package: decision.package_id.clone(),
						bump: decision.recommended_bump,
						trigger: decision.trigger_type.clone(),
						planned_version: decision.planned_version.as_ref().map(ToString::to_string),
						reasons: decision.reasons.clone(),
						upstream_sources: decision.upstream_sources.clone(),
					}
				})
				.collect(),
			groups: prepared_release
				.plan
				.groups
				.iter()
				.map(|group| {
					ReleaseManifestPlanGroup {
						id: group.group_id.clone(),
						planned_version: group.planned_version.as_ref().map(ToString::to_string),
						members: group.members.clone(),
						bump: group.recommended_bump,
					}
				})
				.collect(),
			warnings: prepared_release.plan.warnings.clone(),
			unresolved_items: prepared_release.plan.unresolved_items.clone(),
			compatibility_evidence: prepared_release
				.plan
				.compatibility_evidence
				.iter()
				.map(|assessment| {
					ReleaseManifestCompatibilityEvidence {
						package: assessment.package_id.clone(),
						provider: assessment.provider_id.clone(),
						severity: assessment.severity,
						summary: assessment.summary.clone(),
						confidence: assessment.confidence.clone(),
						evidence_location: assessment.evidence_location.clone(),
					}
				})
				.collect(),
		},
	}
}

pub(crate) fn build_release_manifest_from_record(record: &ReleaseRecord) -> ReleaseManifest {
	ReleaseManifest {
		command: record.command.clone(),
		dry_run: false,
		values: record.values.clone(),
		labels: record.labels.clone(),
		label_inputs: record.label_inputs.clone(),
		version: record.version.clone(),
		group_version: None,
		release_targets: record
			.release_targets
			.iter()
			.map(|target| {
				ReleaseManifestTarget {
					id: target.id.clone(),
					kind: target.kind,
					version: target.version.clone(),
					tag: target.tag,
					release: target.release,
					version_format: target.version_format.clone(),
					tag_name: target.tag_name.clone(),
					members: target.members.clone(),
					rendered_title: record_release_title(record, target),
					rendered_changelog_title: target.rendered_changelog_title.clone(),
					floating_tags: target.floating_tags.clone(),
				}
			})
			.collect(),
		released_packages: record.released_packages.clone(),
		changed_files: record.changed_files.clone(),
		changelogs: if record.changelogs.is_empty() {
			record
				.updated_changelogs
				.iter()
				.map(|path| {
					ReleaseManifestChangelog {
						owner_id: String::new(),
						owner_kind: ReleaseOwnerKind::Group,
						output: DEFAULT_CHANGELOG_OUTPUT.to_owned(),
						stream: DEFAULT_CHANGELOG_STREAM.to_owned(),
						path: path.clone(),
						format: ChangelogFormat::default(),
						notes: ReleaseNotesDocument {
							title: String::new(),
							summary: Vec::new(),
							sections: Vec::new(),
						},
						rendered: String::new(),
					}
				})
				.collect()
		} else {
			record.changelogs.clone()
		},
		package_publications: record.package_publications.clone(),
		changesets: record.changesets.clone(),
		deleted_changesets: record.deleted_changesets.clone(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
	}
}

/// Release title for a record target, replaying the title rendered at
/// prepare time.
///
/// Records that predate persisted titles (schema v0.8 and earlier) carry an
/// empty `rendered_title`; those synthesize the built-in default title for
/// the target's version format, dated from the record's creation, so the
/// provider release name still differs from the bare tag name.
fn record_release_title(record: &ReleaseRecord, target: &ReleaseRecordTarget) -> String {
	if !target.rendered_title.is_empty() {
		return target.rendered_title.clone();
	}
	let created_at = chrono::DateTime::parse_from_rfc3339(&record.created_at)
		.map_or_else(|_| resolve_release_datetime(), |parsed| parsed.naive_utc());
	TitleRenderContext::with_datetime(
		&target.id,
		&target.version,
		record.changesets.len(),
		None,
		&target.tag_name,
		None,
		created_at,
	)
	.render(default_release_title_for_format(&target.version_format))
}

fn release_record_versions(release_targets: &[ReleaseManifestTarget]) -> BTreeMap<String, String> {
	release_targets
		.iter()
		.map(|target| (target.id.clone(), target.version.clone()))
		.collect()
}

pub(crate) fn build_release_record(
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> ReleaseRecord {
	ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION.to_string(),
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: resolve_release_datetime()
			.and_utc()
			.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
		command: manifest.command.clone(),
		version: manifest.version.clone(),
		versions: release_record_versions(&manifest.release_targets),
		release_targets: manifest
			.release_targets
			.iter()
			.map(|target| {
				ReleaseRecordTarget {
					id: target.id.clone(),
					kind: target.kind,
					version: target.version.clone(),
					version_format: target.version_format.clone(),
					tag: target.tag,
					release: target.release,
					tag_name: target.tag_name.clone(),
					rendered_title: target.rendered_title.clone(),
					rendered_changelog_title: target.rendered_changelog_title.clone(),
					members: target.members.clone(),
					floating_tags: target.floating_tags.clone(),
				}
			})
			.collect(),
		released_packages: manifest.released_packages.clone(),
		changed_files: manifest.changed_files.clone(),
		package_publications: manifest.package_publications.clone(),
		updated_changelogs: manifest
			.changelogs
			.iter()
			.map(|changelog| changelog.path.clone())
			.collect(),
		changelogs: manifest.changelogs.clone(),
		deleted_changesets: manifest.deleted_changesets.clone(),
		changesets: manifest.changesets.clone(),
		values: manifest.values.clone(),
		labels: manifest.labels.clone(),
		label_inputs: manifest.label_inputs.clone(),
		provider: source.map(|source| {
			ReleaseRecordProvider {
				kind: source.provider,
				owner: source.owner.clone(),
				repo: source.repo.clone(),
				host: source.host.clone(),
			}
		}),
	}
}

pub(crate) fn build_release_commit_message(
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> CommitMessage {
	CommitMessage {
		subject: source.map_or_else(
			|| monochange_core::ProviderMergeRequestSettings::default().effective_commit_subject(),
			|source| source.pull_requests.effective_commit_subject(),
		),
		body: Some(render_release_commit_body(source, manifest)),
	}
}

pub(crate) fn render_release_commit_body(
	_source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> String {
	let mut lines = vec!["Prepare release.".to_string()];
	if !manifest.release_targets.is_empty() {
		lines.push(String::new());
		lines.push(format!(
			"- release targets: {}",
			manifest
				.release_targets
				.iter()
				.map(|target| format!("{} ({})", target.id, target.version))
				.collect::<Vec<_>>()
				.join(", ")
		));
	}
	if !manifest.released_packages.is_empty() {
		lines.push(format!(
			"- released packages: {}",
			manifest.released_packages.join(", ")
		));
	}
	if !manifest.changelogs.is_empty() {
		lines.push(format!(
			"- updated changelogs: {}",
			manifest
				.changelogs
				.iter()
				.map(|changelog| changelog.path.display().to_string())
				.collect::<Vec<_>>()
				.join(", ")
		));
	}
	if !manifest.deleted_changesets.is_empty() {
		lines.push(format!(
			"- deleted changesets: {}",
			manifest
				.deleted_changesets
				.iter()
				.map(|path| path.display().to_string())
				.collect::<Vec<_>>()
				.join(", ")
		));
	}
	lines.join("\n")
}

#[must_use = "the manifest render result must be checked"]
pub(crate) fn render_release_manifest_json(
	format: OutputFormat,
	manifest: &ReleaseManifest,
) -> MonochangeResult<String> {
	format.render_json_value(manifest, "release manifest")
}

pub(crate) fn build_source_release_requests(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> Vec<SourceReleaseRequest> {
	hosted_sources::hosted_source_adapter(source.provider).build_release_requests(source, manifest)
}

pub(crate) fn build_source_change_request(
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
) -> SourceChangeRequest {
	let mut request = hosted_sources::hosted_source_adapter(source.provider)
		.build_release_pull_request_request(source, manifest);
	request.commit_message = build_release_commit_message(Some(source), manifest);
	request
}

pub(crate) async fn publish_source_release_requests(
	source: &SourceConfiguration,
	requests: &[SourceReleaseRequest],
) -> MonochangeResult<Vec<SourceReleaseOutcome>> {
	match source.provider {
		#[cfg(feature = "github")]
		SourceProvider::GitHub => github_provider::publish_release_requests(source, requests).await,
		#[cfg(feature = "gitlab")]
		SourceProvider::GitLab => gitlab_provider::publish_release_requests(source, requests).await,
		#[cfg(feature = "gitea")]
		SourceProvider::Gitea => gitea_provider::publish_release_requests(source, requests).await,
		#[cfg(feature = "forgejo")]
		SourceProvider::Forgejo => forgejo_provider::publish_release_requests(source, requests).await,
		#[cfg(not(any(
			feature = "github",
			feature = "gitlab",
			feature = "gitea",
			feature = "forgejo"
		)))]
		_ => Ok(Vec::new()),
	}
}

pub(crate) async fn publish_source_change_request(
	source: &SourceConfiguration,
	root: &Path,
	request: &SourceChangeRequest,
	tracked_paths: &[PathBuf],
	no_verify: bool,
	stage_all: bool,
) -> MonochangeResult<SourceChangeRequestOutcome> {
	match source.provider {
		#[cfg(feature = "github")]
		SourceProvider::GitHub => {
			github_provider::publish_release_pull_request(
				source,
				root,
				request,
				tracked_paths,
				no_verify,
				stage_all,
			)
			.await
		}
		#[cfg(feature = "gitlab")]
		SourceProvider::GitLab => {
			gitlab_provider::publish_release_pull_request(
				source,
				root,
				request,
				tracked_paths,
				no_verify,
				stage_all,
			)
			.await
		}
		#[cfg(feature = "gitea")]
		SourceProvider::Gitea => {
			gitea_provider::publish_release_pull_request(
				source,
				root,
				request,
				tracked_paths,
				no_verify,
				stage_all,
			)
			.await
		}
		#[cfg(feature = "forgejo")]
		SourceProvider::Forgejo => {
			forgejo_provider::publish_release_pull_request(
				source,
				root,
				request,
				tracked_paths,
				no_verify,
				stage_all,
			)
			.await
		}
		#[cfg(not(any(
			feature = "github",
			feature = "gitlab",
			feature = "gitea",
			feature = "forgejo"
		)))]
		_ => {
			Err(MonochangeError::Config(
				"no hosting provider feature enabled".to_string(),
			))
		}
	}
}

pub(crate) fn format_source_operation(operation: &SourceReleaseOperation) -> &'static str {
	match operation {
		SourceReleaseOperation::Created => "created",
		SourceReleaseOperation::Updated => "updated",
	}
}

pub(crate) fn format_change_request_operation(
	operation: &SourceChangeRequestOperation,
) -> &'static str {
	match operation {
		SourceChangeRequestOperation::Created => "created",
		SourceChangeRequestOperation::Updated => "updated",
		SourceChangeRequestOperation::Skipped => "skipped",
	}
}

pub(crate) struct ReleaseCliJsonSections<'a> {
	pub releases: &'a [SourceReleaseRequest],
	pub release_request: Option<&'a SourceChangeRequest>,
	pub issue_comments: &'a [HostedIssueCommentPlan],
	pub release_commit: Option<&'a CommitReleaseReport>,
	pub package_publish: Option<&'a package_publish::PackagePublishReport>,
	pub publish_rate_limits: Option<&'a monochange_core::PublishRateLimitReport>,
	pub file_diffs: &'a [PreparedFileDiff],
	pub commands: &'a [CommandStepResult],
}

pub(crate) fn render_release_cli_command_json(
	format: OutputFormat,
	manifest: &ReleaseManifest,
	sections: &ReleaseCliJsonSections,
) -> MonochangeResult<String> {
	if sections.releases.is_empty()
		&& sections.release_request.is_none()
		&& sections.issue_comments.is_empty()
		&& sections.release_commit.is_none()
		&& sections.package_publish.is_none()
		&& sections.publish_rate_limits.is_none()
		&& sections.file_diffs.is_empty()
	{
		if sections.commands.is_empty() {
			return render_release_manifest_json(format, manifest);
		}
		// Keep the manifest shape and add `commands` beside its fields, so
		// consumers reading manifest fields at the top level keep working.
		let mut value = json!(manifest);
		value
			.as_object_mut()
			.unwrap_or_else(|| panic!("release manifest json must stay object"))
			.insert("commands".to_string(), json!(sections.commands));
		return format.render_json_value(&value, "release manifest");
	}
	let mut value = json!({
		"manifest": manifest,
		"release_commit": sections.release_commit,
		"releases": sections.releases,
		"release_request": sections.release_request,
		"issue_comments": sections.issue_comments,
		"package_publish": sections.package_publish,
		"publish_rate_limits": sections.publish_rate_limits,
	});
	if !sections.file_diffs.is_empty() {
		value
			.as_object_mut()
			.unwrap_or_else(|| panic!("release json wrapper must stay object"))
			.insert(
				"file_diffs".to_string(),
				serde_json::to_value(sections.file_diffs).unwrap_or_default(),
			);
	}
	if !sections.commands.is_empty() {
		value
			.as_object_mut()
			.unwrap_or_else(|| panic!("release json wrapper must stay object"))
			.insert("commands".to_string(), json!(sections.commands));
	}
	format.render_json_value(&value, "release command output")
}

pub(crate) fn write_release_record_file(
	root: &Path,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
) -> MonochangeResult<PathBuf> {
	let paths = ReleasePaths::from_manifest(root, manifest);

	// If the record already exists, return it without overwriting so that
	// subsequent PrepareRelease steps (for example during `monochange release-pr`)
	// do not produce a dirty working tree.
	if paths.absolute.is_file() {
		add_to_dedup_index(root, &paths.hash).ok();
		return Ok(paths.absolute);
	}

	let record = build_release_record(source, manifest);
	deduplicate_overlapping_release_records(
		root,
		&record.release_targets,
		paths.absolute.parent().unwrap_or(root),
	)?;
	let json = serde_json::to_string_pretty(&record).unwrap_or_default();
	fs::create_dir_all(paths.absolute.parent().unwrap_or(root))
		.map_err(|error| MonochangeError::Io(format!("create release record dir: {error}")))?;
	fs::write(&paths.absolute, json)
		.map_err(|error| MonochangeError::Io(format!("write release record: {error}")))?;
	add_to_dedup_index(root, &paths.hash)?;
	Ok(paths.absolute)
}

/// Compare two JSON strings for semantic equality.
/// If the strings are byte-equal, they match immediately.
/// Otherwise, both are parsed into `serde_json::Value` and compared structurally.
fn compare_json_strings(json1: &str, json2: &str) -> bool {
	if json1 == json2 {
		return true;
	}

	let parsed1: Result<serde_json::Value, _> = serde_json::from_str(json1);
	let parsed2: Result<serde_json::Value, _> = serde_json::from_str(json2);

	match (parsed1, parsed2) {
		(Ok(v1), Ok(v2)) => v1 == v2,
		_ => false,
	}
}

/// Validate that the release record file expected for `manifest` still exists
/// on disk after re-running deduplication. Called by `commit_release` to
/// guard against stale or missing records between `prepare_release` and
/// `commit_release`.
///
/// Performance note: when the file already exists and its `release_targets`
/// match the manifest, this function skips rebuilding the `ReleaseRecord`
/// entirely. It only falls back to `build_release_record` when the file is
/// missing or the targets differ, avoiding the JSON round-trip on the hot
/// path.
pub(crate) fn validate_release_record_file(
	root: &Path,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
	update_release_json: bool,
) -> MonochangeResult<PathBuf> {
	// Compute the expected path from the manifest without building the record.
	let paths = ReleasePaths::from_manifest(root, manifest);

	// Fast path: if the file exists, verify its release_targets identity
	// without rebuilding the entire ReleaseRecord.
	if paths.absolute.is_file() {
		match fs::read_to_string(&paths.absolute) {
			Ok(existing_json) => {
				if let Ok(existing_value) =
					serde_json::from_str::<serde_json::Value>(&existing_json)
					&& let Some(existing_targets) = existing_value
						.get("release_targets")
						.and_then(|v| v.as_array())
				{
					let manifest_targets = &manifest.release_targets;
					if existing_targets.len() == manifest_targets.len() {
						let all_match = manifest_targets.iter().all(|mt| {
							existing_targets.iter().any(|et| {
								let Some(id) = et.get("id").and_then(|v| v.as_str()) else {
									return false;
								};
								let Some(kind) = et.get("kind").and_then(|v| v.as_str()) else {
									return false;
								};
								let Some(version) = et.get("version").and_then(|v| v.as_str())
								else {
									return false;
								};
								mt.id == id && mt.kind.as_str() == kind && mt.version == version
							})
						});
						if all_match {
							// Targets match — no need to rebuild or rewrite.
							add_to_dedup_index(root, &paths.hash).ok();
							return Ok(paths.absolute);
						}
					}
				}
			}
			Err(error) => {
				return Err(MonochangeError::Io(format!("read release record: {error}")));
			}
		}
	}

	// Slow path: rebuild the record, deduplicate, and validate or rewrite.
	let record = build_release_record(source, manifest);
	deduplicate_overlapping_release_records(
		root,
		&record.release_targets,
		paths.absolute.parent().unwrap_or(root),
	)?;
	let json = serde_json::to_string_pretty(&record).unwrap_or_default();
	if paths.absolute.is_file() {
		let existing = fs::read_to_string(&paths.absolute)
			.map_err(|error| MonochangeError::Io(format!("read release record: {error}")))?;
		if !compare_json_strings(&existing, &json) {
			if update_release_json {
				fs::write(&paths.absolute, json).map_err(|error| {
					MonochangeError::Io(format!("update release record: {error}"))
				})?;
			} else {
				return Err(MonochangeError::Io(format!(
					"release record at {} does not match expected content — the file has been modified since it was prepared. Set `update_release_json = true` on the CommitRelease step to allow overwriting.",
					paths.absolute.display()
				)));
			}
		}
	} else if update_release_json {
		fs::create_dir_all(paths.absolute.parent().unwrap_or(root))
			.map_err(|error| MonochangeError::Io(format!("create release record dir: {error}")))?;
		fs::write(&paths.absolute, json)
			.map_err(|error| MonochangeError::Io(format!("write release record: {error}")))?;
	} else {
		return Err(MonochangeError::Io(format!(
			"no release record found at {} — was it removed by deduplication or never written?",
			paths.absolute.display()
		)));
	}
	Ok(paths.absolute)
}

/// Derived filesystem paths for a release record.
///
/// The record path is a deterministic function of the manifest's
/// `release_targets`. It is computed on demand rather than stored in the
/// manifest so that the manifest remains portable and the path format can
/// evolve without invalidating cached manifests.
#[allow(dead_code)]
pub(crate) struct ReleasePaths {
	/// Hexadecimal hash derived from the release targets.
	pub hash: String,
	/// Path relative to the workspace root (`.monochange/releases/<hash>/release.json`).
	pub relative: PathBuf,
	/// Absolute path resolved against the workspace root.
	pub absolute: PathBuf,
}

/// Compute the record paths for a set of release targets.
///
/// Callers that only have targets (not a full manifest or record) use this to
/// check whether a release record already exists.
pub(crate) fn release_record_paths(root: &Path, targets: &[ReleaseManifestTarget]) -> ReleasePaths {
	ReleasePaths::from_manifest_targets(root, targets)
}

impl ReleasePaths {
	/// Compute paths from an already-built `ReleaseRecord`.
	///
	/// Use this when you have the record in hand to avoid rebuilding it.
	#[allow(dead_code)]
	pub fn from_record(root: &Path, record: &ReleaseRecord) -> Self {
		let hash = release_targets_hash(&record.release_targets);
		let relative = PathBuf::from(".monochange/releases")
			.join(&hash)
			.join("release.json");
		let absolute = root.join(&relative);
		Self {
			hash,
			relative,
			absolute,
		}
	}

	/// Compute paths directly from a `ReleaseManifest`.
	///
	/// This builds the intermediate `ReleaseRecord` internally, so prefer
	/// `from_record` when the record is already available.
	/// Compute paths directly from a `ReleaseManifest`.
	///
	/// Unlike `from_record`, this does **not** build the intermediate
	/// `ReleaseRecord`. The hash is derived from `manifest.release_targets`
	/// directly so callers can check file existence before doing expensive work.
	pub fn from_manifest(root: &Path, manifest: &ReleaseManifest) -> Self {
		Self::from_manifest_targets(root, &manifest.release_targets)
	}

	/// Compute paths from release targets alone.
	pub fn from_manifest_targets(root: &Path, release_targets: &[ReleaseManifestTarget]) -> Self {
		let hash = release_targets_hash(release_targets);
		let relative = PathBuf::from(".monochange/releases")
			.join(&hash)
			.join("release.json");
		let absolute = root.join(&relative);
		Self {
			hash,
			relative,
			absolute,
		}
	}
}

/// Identity-aware hash for a slice of release targets.
///
/// The hash is deterministic: targets are sorted by `(id, kind, version)`
/// before hashing so that manifest order never affects the path.
///
/// Fields included in the hash: `id`, `kind`, `version`.
/// Excluded: `tag`, `release`, `tag_name`, `version_format`, `members`.
fn release_targets_hash<T: ReleaseTargetIdentity>(release_targets: &[T]) -> String {
	use std::collections::hash_map::DefaultHasher;
	use std::hash::Hasher;
	let mut hasher = DefaultHasher::new();
	let mut sorted: Vec<&T> = release_targets.iter().collect();
	sorted.sort_by(|a, b| {
		a.id()
			.cmp(b.id())
			.then_with(|| a.kind().as_str().cmp(b.kind().as_str()))
			.then_with(|| a.version().cmp(b.version()))
	});
	for target in sorted {
		hasher.write(target.id().as_bytes());
		hasher.write(target.kind().as_str().as_bytes());
		hasher.write(target.version().as_bytes());
	}
	format!("{:016x}", hasher.finish())
}

/// Trait exposing the identity fields that participate in the release-target
/// hash. Implemented for both `ReleaseManifestTarget` and `ReleaseRecordTarget`
/// so that `release_targets_hash` can work with either slice type.
trait ReleaseTargetIdentity {
	fn id(&self) -> &str;
	fn kind(&self) -> ReleaseOwnerKind;
	fn version(&self) -> &str;
}

impl ReleaseTargetIdentity for ReleaseManifestTarget {
	fn id(&self) -> &str {
		&self.id
	}

	fn kind(&self) -> ReleaseOwnerKind {
		self.kind
	}

	fn version(&self) -> &str {
		&self.version
	}
}

impl ReleaseTargetIdentity for ReleaseRecordTarget {
	fn id(&self) -> &str {
		&self.id
	}

	fn kind(&self) -> ReleaseOwnerKind {
		self.kind
	}

	fn version(&self) -> &str {
		&self.version
	}
}

fn deduplicate_overlapping_release_records(
	root: &Path,
	release_targets: &[ReleaseRecordTarget],
	current_record_dir: &Path,
) -> MonochangeResult<()> {
	let hash = release_targets_hash(release_targets);
	let already_deduped = DEDUPLICATED_CACHE
		.with(|cache| cache.borrow().contains(&(root.to_path_buf(), hash.clone())));
	if already_deduped {
		return Ok(());
	}

	let persistent_index = load_dedup_index(root);
	if persistent_index.contains(&hash) {
		DEDUPLICATED_CACHE.with(|cache| {
			cache
				.borrow_mut()
				.insert((root.to_path_buf(), hash.clone()));
		});
		return Ok(());
	}

	let new_tags: std::collections::HashSet<(&str, &str)> = release_targets
		.iter()
		.map(|target| (target.id.as_str(), target.version.as_str()))
		.collect();

	let releases_dir = root.join(".monochange/releases");
	if !releases_dir.is_dir() {
		return Ok(());
	}
	for entry in fs::read_dir(&releases_dir)
		.map_err(|error| MonochangeError::Io(format!("read releases dir: {error}")))?
	{
		let entry =
			entry.map_err(|error| MonochangeError::Io(format!("read dir entry: {error}")))?;
		let path = entry.path();
		if path == current_record_dir {
			continue;
		}
		if !path.is_dir() {
			continue;
		}
		let record_file = path.join("release.json");
		if !record_file.is_file() {
			continue;
		}
		let Ok(content) = fs::read_to_string(&record_file) else {
			continue;
		};
		let Ok(existing) = serde_json::from_str::<ReleaseRecord>(&content) else {
			continue;
		};
		let has_overlap = existing
			.release_targets
			.iter()
			.any(|t| new_tags.contains(&(t.id.as_str(), t.version.as_str())));
		if has_overlap {
			let hash_to_remove = release_targets_hash(&existing.release_targets);
			fs::remove_dir_all(&path).map_err(|error| {
				MonochangeError::Io(format!("remove stale release record dir: {error}"))
			})?;
			remove_from_dedup_index(root, &hash_to_remove).ok();
		}
	}

	DEDUPLICATED_CACHE.with(|cache| {
		cache
			.borrow_mut()
			.insert((root.to_path_buf(), hash.clone()));
	});
	add_to_dedup_index(root, &hash)?;

	Ok(())
}

pub(crate) async fn commit_release(
	root: &Path,
	context: &CliContext,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
	no_verify: bool,
	update_release_json: bool,
	stage_all: bool,
) -> MonochangeResult<CommitReleaseReport> {
	let prepared = prepare_release_commit(root, context, source, manifest, update_release_json)?;
	if !context.dry_run {
		// patch-coverage:ignore-start -- exercised by end-to-end release PR flows; branch delegates to covered git helpers.
		if stage_all {
			git_stage_all(root).await?;
		} else {
			git_stage_paths(root, &prepared.tracked_paths).await?;
		}
		// patch-coverage:ignore-end
		git_commit_paths(root, &prepared.message, no_verify).await?;
	}
	Ok(CommitReleaseReport {
		subject: prepared.message.subject,
		body: prepared.message.body.clone().unwrap_or_default(),
		commit: if context.dry_run {
			None
		} else {
			Some(git_head_commit(root).await?)
		},
		verified: None,
		tracked_paths: prepared.tracked_paths,
		dry_run: context.dry_run,
		status: if context.dry_run {
			"dry_run".to_string()
		} else {
			"completed".to_string()
		},
	})
}

/// Everything both commit backends need after local preparation.
struct PreparedReleaseCommit {
	message: CommitMessage,
	tracked_paths: Vec<PathBuf>,
}

/// Validate the release record and collect the commit message and tracked
/// paths shared by the local and hosted commit backends.
fn prepare_release_commit(
	root: &Path,
	context: &CliContext,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
	update_release_json: bool,
) -> MonochangeResult<PreparedReleaseCommit> {
	let tracked_paths = tracked_release_pull_request_paths(context, manifest);
	let message = build_release_commit_message(source, manifest);
	// Dry-run previews report the commit they would make. `PrepareRelease`
	// skips writing the release record in dry-run mode, so requiring it here
	// failed every `run <workflow> --dry-run` that chains the two steps.
	// Report the expected record path instead; the real run still validates
	// (or writes) the record.
	let release_record_path = if context.dry_run {
		release_record_paths(root, &manifest.release_targets).absolute
	} else {
		validate_release_record_file(root, source, manifest, update_release_json)?
	};
	let mut tracked_paths = tracked_paths;
	tracked_paths.push(release_record_path);
	Ok(PreparedReleaseCommit {
		message,
		tracked_paths,
	})
}

/// Resolved configuration for the hosted commit backend.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct HostedCommitOptions {
	/// Authentication mode resolved from the step's `hosted_auth` setting.
	pub(crate) auth: monochange_core::HostedCommitAuth,
	/// Monochange app base URL without a trailing slash.
	pub(crate) url: String,
	/// Audience for the GitHub Actions OIDC token.
	pub(crate) oidc_audience: String,
}

pub(crate) const DEFAULT_HOSTED_URL: &str = "https://monochange.dev";

/// Hosted backend settings configured on a `CommitRelease` or
/// `OpenReleaseRequest` step in `monochange.toml`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConfiguredHostedSettings<'a> {
	pub(crate) auth: monochange_core::HostedCommitAuth,
	pub(crate) url: Option<&'a str>,
	pub(crate) oidc_audience: Option<&'a str>,
}

/// Resolve the hosted backend options for one step invocation.
///
/// Step inputs (command-line flags or workflow `inputs`) win over the
/// environment, which wins over the step configuration. Only the URL has an
/// environment override, `MONOCHANGE_HOSTED_URL`, so self-hosted monochange
/// app deployments can redirect the backend without editing monochange.toml.
/// Empty values are ignored so unset workflow inputs never shadow the
/// configuration.
///
/// # Errors
///
/// Returns an error when the `hosted_auth` input is not a supported mode.
pub(crate) fn hosted_commit_options_from_step(
	step_inputs: &BTreeMap<String, Vec<String>>,
	configured: ConfiguredHostedSettings<'_>,
) -> MonochangeResult<HostedCommitOptions> {
	let step_input =
		|name: &str| string_step_input(step_inputs, name).filter(|value| !value.is_empty());
	let auth = match step_input("hosted_auth") {
		Some(value) => value.parse()?,
		None => configured.auth,
	};
	let url = step_input("hosted_url")
		.or_else(|| {
			std::env::var("MONOCHANGE_HOSTED_URL")
				.ok()
				.filter(|value| !value.is_empty())
		})
		.or_else(|| configured.url.map(String::from));
	let oidc_audience =
		step_input("oidc_audience").or_else(|| configured.oidc_audience.map(String::from));
	Ok(resolve_hosted_commit_options(
		auth,
		url.as_deref(),
		oidc_audience.as_deref(),
	))
}

/// Build hosted commit options from resolved values, falling back to the
/// public app and an audience derived from its host.
#[must_use]
pub(crate) fn resolve_hosted_commit_options(
	auth: monochange_core::HostedCommitAuth,
	url: Option<&str>,
	oidc_audience: Option<&str>,
) -> HostedCommitOptions {
	let url = url
		.unwrap_or(DEFAULT_HOSTED_URL)
		.trim_end_matches('/')
		.to_string();
	let oidc_audience =
		oidc_audience.map_or_else(|| hosted_oidc_audience(&url), ToString::to_string);
	HostedCommitOptions {
		auth,
		url,
		oidc_audience,
	}
}

/// Derive the default OIDC audience from the hosted app URL host.
#[must_use]
fn hosted_oidc_audience(url: &str) -> String {
	url.trim_start_matches("https://")
		.trim_start_matches("http://")
		.trim_end_matches('/')
		.to_string()
}

/// Send the prepared release files to the monochange app, which commits
/// through the monochange GitHub App installation.
///
/// The release is still computed locally; only the commit is delegated so
/// the resulting commit is created by the monochange bot identity, is
/// verified by GitHub, and triggers the repository's normal workflows.
pub(crate) async fn hosted_commit_release(
	root: &Path,
	context: &CliContext,
	source: Option<&SourceConfiguration>,
	manifest: &ReleaseManifest,
	update_release_json: bool,
	options: &HostedCommitOptions,
) -> MonochangeResult<CommitReleaseReport> {
	let prepared = prepare_release_commit(root, context, source, manifest, update_release_json)?;
	let body = prepared.message.body.clone().unwrap_or_default();
	let branches = hosted_commit_branches(source, &manifest.command);
	let request = build_hosted_commit_request(root, &prepared, context.dry_run, &branches).await?;
	let response = if context.dry_run {
		HostedCommitResponse {
			commit: None,
			verified: false,
			status: Some("dry_run".to_string()),
			message: None,
		}
	} else {
		let token = hosted_commit_bearer_token(options).await?;
		send_hosted_commit_request(&request, options, &token).await?
	};
	Ok(CommitReleaseReport {
		subject: prepared.message.subject,
		body,
		commit: response.commit,
		verified: (!context.dry_run).then_some(response.verified),
		tracked_paths: prepared.tracked_paths,
		dry_run: context.dry_run,
		status: response.status.unwrap_or_else(|| "completed".to_string()),
	})
}

/// The branches a hosted release commit is written to and prepared from.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct HostedCommitBranches {
	/// Release branch the commit lands on; the release request's head branch.
	pub(crate) release: String,
	/// Branch the release was prepared from; the release request's base branch.
	pub(crate) base: String,
}

/// Resolve the hosted commit branches from the `[source.pull_requests]`
/// settings, so the commit lands on the branch `OpenReleaseRequest` opens.
///
/// The branch a workflow was triggered from is never the target: committing
/// there would put the release commit on `main` instead of the release
/// pull request.
#[must_use]
pub(crate) fn hosted_commit_branches(
	source: Option<&SourceConfiguration>,
	command: &str,
) -> HostedCommitBranches {
	let defaults = monochange_core::ProviderMergeRequestSettings::default();
	let pull_requests = source.map_or(&defaults, |source| &source.pull_requests);
	HostedCommitBranches {
		release: monochange_hosting::release_pull_request_branch(
			&pull_requests.branch_prefix,
			command,
		),
		base: pull_requests.base.clone(),
	}
}

/// Build the provider-neutral request the monochange app commits on our behalf.
async fn build_hosted_commit_request(
	root: &Path,
	prepared: &PreparedReleaseCommit,
	dry_run: bool,
	branches: &HostedCommitBranches,
) -> MonochangeResult<HostedCommitRequest> {
	let repository = std::env::var("GITHUB_REPOSITORY").map_err(|_| {
		MonochangeError::Config(
			"hosted CommitRelease requires GITHUB_REPOSITORY (for example `owner/repo`)"
				.to_string(),
		)
	})?;
	build_hosted_commit_request_for_github(root, prepared, dry_run, &repository, branches).await
}

/// Build the hosted commit request for an explicit repository, so tests do
/// not need GitHub Actions environment variables.
async fn build_hosted_commit_request_for_github(
	root: &Path,
	prepared: &PreparedReleaseCommit,
	dry_run: bool,
	repository: &str,
	branches: &HostedCommitBranches,
) -> MonochangeResult<HostedCommitRequest> {
	let (owner, repository) = repository.split_once('/').ok_or_else(|| {
		MonochangeError::Config("GITHUB_REPOSITORY must use `owner/repo` format".to_string())
	})?;
	let files = prepared
		.tracked_paths
		.iter()
		.map(|path| hosted_commit_file(root, path))
		.collect::<MonochangeResult<Vec<_>>>()?;
	Ok(HostedCommitRequest {
		provider: "github".to_string(),
		owner: owner.to_string(),
		repository: repository.to_string(),
		branch: branches.release.clone(),
		base_branch: Some(branches.base.clone()),
		base_commit: git_head_commit(root).await?,
		subject: prepared.message.subject.clone(),
		body: prepared.message.body.clone().unwrap_or_default(),
		files,
		dry_run,
		idempotency_key: hosted_commit_idempotency_key(),
	})
}

/// Build a retry-safe idempotency key from the current GitHub Actions run.
#[must_use]
fn hosted_commit_idempotency_key() -> Option<String> {
	// patch-coverage:ignore-start -- the happy path needs GITHUB_REPOSITORY plus run metadata; writing those process-global vars races unlocked readers in parallel suites. The integration suite exercises it end-to-end through the spawned CLI.
	let repository = std::env::var("GITHUB_REPOSITORY").ok()?;
	let run_id = std::env::var("GITHUB_RUN_ID").ok()?;
	let attempt = std::env::var("GITHUB_RUN_ATTEMPT").ok()?;
	Some(format!("{repository}:{run_id}:{attempt}:CommitRelease"))
	// patch-coverage:ignore-end
}

/// Read one release-managed file; missing content deletes the path, which is
/// how consumed changeset files are removed from the release commit.
///
/// Tracked paths are usually workspace-relative, but the release record is
/// tracked by its absolute path; the app only accepts repository-relative
/// paths, so absolute paths are made relative to `root`.
fn hosted_commit_file(root: &Path, path: &Path) -> MonochangeResult<HostedCommitFile> {
	let path = path.strip_prefix(root).unwrap_or(path);
	let full_path = root.join(path);
	let content = if full_path.exists() {
		Some(fs::read_to_string(&full_path).map_err(|error| {
			MonochangeError::Io(format!(
				"read hosted commit file `{}`: {error}",
				path.display()
			))
		})?)
	} else {
		None
	};
	Ok(HostedCommitFile {
		path: path.to_string_lossy().replace('\\', "/"),
		content,
	})
}

/// POST the hosted commit request and validate the response.
async fn send_hosted_commit_request(
	request: &HostedCommitRequest,
	options: &HostedCommitOptions,
	token: &str,
) -> MonochangeResult<HostedCommitResponse> {
	let base_url = options.url.trim_end_matches('/');
	let client = monochange_hosting::build_http_client("monochange app")?;
	let response = client
		.post(format!("{base_url}/api/release-commits"))
		.bearer_auth(token)
		.json(request)
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("hosted CommitRelease request failed: {error}"))
		})?;
	let status = response.status();
	// patch-coverage:ignore-start -- a mid-body connection drop is needed to fail the text read; httpmock always serves complete bodies.
	let text = response.text().await.map_err(|error| {
		MonochangeError::Config(format!(
			"hosted CommitRelease response read failed: {error}"
		))
	})?;
	// patch-coverage:ignore-end
	if !status.is_success() {
		return Err(MonochangeError::Config(format!(
			"hosted CommitRelease failed with HTTP {status}: {text}"
		)));
	}
	serde_json::from_str(&text).map_err(|error| {
		MonochangeError::Config(format!(
			"hosted CommitRelease response was invalid JSON: {error}"
		))
	})
}

/// Ask the monochange app to open or refresh the release pull request through
/// the monochange GitHub App installation.
///
/// The request body, title, labels, and branches are computed locally; the
/// server creates or updates the pull request under the bot identity so the
/// pull request events trigger the repository's normal workflows.
pub(crate) async fn hosted_release_request_result(
	dry_run: bool,
	options: &HostedCommitOptions,
	request: &SourceChangeRequest,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<String> {
	if dry_run {
		return build_release_request_result(dry_run, request, || unreachable!());
	}
	let token = hosted_commit_bearer_token(options).await?;
	let payload = HostedReleaseRequest {
		request: request.clone(),
		tracked_paths: tracked_paths
			.iter()
			.map(|path| path.to_string_lossy().replace('\\', "/"))
			.collect(),
		dry_run,
	};
	let base_url = options.url.trim_end_matches('/');
	let client = monochange_hosting::build_http_client("monochange app")?;
	let response = client
		.post(format!("{base_url}/api/release-requests"))
		.bearer_auth(token)
		.json(&payload)
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!("hosted OpenReleaseRequest failed: {error}"))
		})?;
	let status = response.status();
	// patch-coverage:ignore-start -- a mid-body connection drop is needed to fail the text read; httpmock always serves complete bodies.
	let text = response.text().await.map_err(|error| {
		MonochangeError::Config(format!(
			"hosted OpenReleaseRequest response read failed: {error}"
		))
	})?;
	// patch-coverage:ignore-end
	if !status.is_success() {
		return Err(MonochangeError::Config(format!(
			"hosted OpenReleaseRequest failed with HTTP {status}: {text}"
		)));
	}
	let outcome: HostedReleaseResponse = serde_json::from_str(&text).map_err(|error| {
		MonochangeError::Config(format!(
			"hosted OpenReleaseRequest response was invalid JSON: {error}"
		))
	})?;
	let repository = &request.repository;
	Ok(match (outcome.number, outcome.operation) {
		(Some(number), Some(operation)) => {
			format!(
				"{repository} #{number} ({}) via monochange app",
				format_change_request_operation(&operation)
			)
		}
		_ => format!("{repository} (no pull request) via monochange app"),
	})
}

/// Resolve the bearer token for the monochange app request.
async fn hosted_commit_bearer_token(options: &HostedCommitOptions) -> MonochangeResult<String> {
	match options.auth {
		monochange_core::HostedCommitAuth::Token => hosted_monochange_token(),
		monochange_core::HostedCommitAuth::Oidc => hosted_github_actions_oidc_token(options).await,
		monochange_core::HostedCommitAuth::Auto => {
			if std::env::var_os("ACTIONS_ID_TOKEN_REQUEST_URL").is_some() {
				hosted_github_actions_oidc_token(options).await
			} else {
				hosted_monochange_token()
			}
		}
	}
}

fn hosted_monochange_token() -> MonochangeResult<String> {
	std::env::var("MONOCHANGE_TOKEN").map_err(|_| {
		MonochangeError::Config(
			"hosted CommitRelease token auth requires MONOCHANGE_TOKEN".to_string(),
		)
	})
}

#[derive(serde::Deserialize)]
struct GithubActionsOidcResponse {
	value: String,
}

/// Request a GitHub Actions OIDC token for the configured audience.
async fn hosted_github_actions_oidc_token(
	options: &HostedCommitOptions,
) -> MonochangeResult<String> {
	let request_url = std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL").map_err(|_| {
		MonochangeError::Config(
			"hosted CommitRelease OIDC auth requires ACTIONS_ID_TOKEN_REQUEST_URL".to_string(),
		)
	})?;
	let request_token = std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN").map_err(|_| {
		MonochangeError::Config(
			"hosted CommitRelease OIDC auth requires ACTIONS_ID_TOKEN_REQUEST_TOKEN".to_string(),
		)
	})?;
	let audience = &options.oidc_audience;
	let separator = if request_url.contains('?') { '&' } else { '?' };
	let client = monochange_hosting::build_http_client("monochange app")?;
	let response = client
		.get(format!("{request_url}{separator}audience={audience}"))
		.bearer_auth(request_token)
		.send()
		.await
		.map_err(|error| {
			MonochangeError::Config(format!(
				"hosted CommitRelease OIDC token request failed: {error}"
			))
		})?;
	if !response.status().is_success() {
		return Err(MonochangeError::Config(format!(
			"hosted CommitRelease OIDC token request failed with status {}",
			response.status()
		)));
	}
	response
		.json::<GithubActionsOidcResponse>()
		.await
		.map(|response| response.value)
		.map_err(|error| {
			MonochangeError::Config(format!(
				"hosted CommitRelease OIDC token response was invalid: {error}"
			))
		})
}

pub(crate) fn tracked_release_pull_request_paths(
	context: &CliContext,
	manifest: &ReleaseManifest,
) -> Vec<PathBuf> {
	let mut tracked_paths = manifest.changed_files.clone();
	tracked_paths.extend(manifest.deleted_changesets.clone());
	if let Some(path) = &context.release_manifest_path {
		tracked_paths.push(path.clone());
	}
	tracked_paths.sort();
	tracked_paths.dedup();
	tracked_paths
}

pub(crate) fn json_discovery_report(report: &DiscoveryReport) -> serde_json::Value {
	json!({
		"workspace_root": PathBuf::from("."),
		"packages": report.packages.iter().map(|package| {
			json!({
				"id": package.id,
				"name": package.name,
				"ecosystem": package.ecosystem.as_str(),
				"manifest_path": root_relative(&report.workspace_root, &package.manifest_path),
				"workspace_root": PathBuf::from("."),
				"version": package.current_version.as_ref().map(ToString::to_string),
				"version_group": package.version_group_id,
				"publish_state": format_publish_state(package.publish_state),
			})
		}).collect::<Vec<_>>(),
		"dependencies": report.dependencies.iter().map(|edge| {
			json!({
				"from": edge.from_package_id,
				"to": edge.to_package_id,
				"kind": edge.dependency_kind.to_string(),
				"direct": edge.is_direct,
			})
		}).collect::<Vec<_>>(),
		"version_groups": report.version_groups.iter().map(|group| {
			json!({
				"id": group.group_id,
				"members": group.members,
				"mismatch_detected": group.mismatch_detected,
			})
		}).collect::<Vec<_>>(),
		"warnings": report.warnings,
	})
}

pub(crate) fn text_discovery_report(report: &DiscoveryReport) -> String {
	let mut counts = BTreeMap::<Ecosystem, usize>::new();
	for package in &report.packages {
		*counts.entry(package.ecosystem).or_default() += 1;
	}
	let mut counts = counts.into_iter().collect::<Vec<_>>();
	counts.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));

	let mut text = TextReport::new(TextTheme::for_stdout());
	let outcome = if report.packages.is_empty() {
		Outcome::Neutral
	} else {
		Outcome::Success
	};
	text.headline(
		outcome,
		&format!(
			"Discovered {}",
			plural(report.packages.len(), "package", "packages")
		),
		&[plural(
			report.dependencies.len(),
			"dependency",
			"dependencies",
		)],
	);

	if !counts.is_empty() {
		text.section("Ecosystems", None);
		text.table(
			&counts
				.iter()
				.map(|(ecosystem, count)| {
					vec![
						TableCell::new(ecosystem.to_string(), Tone::Heading),
						TableCell::new(count.to_string(), Tone::Value),
					]
				})
				.collect::<Vec<_>>(),
		);
	}
	if !report.version_groups.is_empty() {
		text.section("Version groups", None);
		text.table(
			&report
				.version_groups
				.iter()
				.map(|group| {
					vec![
						TableCell::new(&group.group_id, Tone::Heading),
						TableCell::new(
							plural(group.members.len(), "package", "packages"),
							Tone::Muted,
						),
					]
				})
				.collect::<Vec<_>>(),
		);
	}
	if !report.warnings.is_empty() {
		let root_prefix = format!("{}/", report.workspace_root.display());
		text.section("Warnings", Some(report.warnings.len()));
		text.list(
			report
				.warnings
				.iter()
				.map(|warning| format!("▲ {}", warning.replace(&root_prefix, ""))),
			usize::MAX,
		);
	}
	text.render()
}

#[cfg(test)]
#[path = "__tests__/release_artifacts_tests.rs"]
mod tests;
