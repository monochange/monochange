use std::collections::HashSet;

use regex::Regex;

use super::*;

/// Context needed when applying version updates to versioned files.
///
/// # Ownership model
///
/// This struct uses a mixed ownership model by design:
///
/// - **Borrowed data** (`&'a str` keys, `&'a PackageRecord` values, `&'a WorkspaceConfiguration`):
///   These references borrow directly from the caller's package records and workspace configuration.
///   No allocation is needed because the underlying data already lives in the caller's scope.
///
/// - **Owned data** (`BTreeMap<String, String>`):
///   These maps contain derived values computed from the release plan (e.g. resolved version strings).
///   They must be owned because they don't exist in the input data and are produced during context
///   construction.
///
/// The `'a` lifetime therefore ties this context to the lifetime of the input data passed by the
/// caller, while the owned maps represent newly computed information that is independent of that
/// borrow.
pub(crate) struct VersionedFileUpdateContext<'a> {
	pub(crate) package_by_config_id: BTreeMap<&'a str, &'a PackageRecord>,
	pub(crate) package_by_native_name: BTreeMap<&'a str, &'a PackageRecord>,
	pub(crate) current_versions_by_native_name: BTreeMap<String, String>,
	pub(crate) released_versions_by_native_name: BTreeMap<String, String>,
	pub(crate) configuration: &'a monochange_core::WorkspaceConfiguration,
}

#[derive(Debug)]
pub(crate) enum CachedDocument {
	Json(serde_json::Value),
	Yaml(serde_yaml_ng::Mapping),
	Text(String),
	Bytes(Vec<u8>),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum VersionedFileKind {
	#[cfg(feature = "cargo")]
	Cargo(monochange_cargo::CargoVersionedFileKind),
	#[cfg(feature = "npm")]
	Npm(monochange_npm::NpmVersionedFileKind),
	#[cfg(feature = "deno")]
	Deno(monochange_deno::DenoVersionedFileKind),
	#[cfg(feature = "dart")]
	Dart(monochange_dart::DartVersionedFileKind),
	#[cfg(feature = "python")]
	Python(monochange_python::PythonVersionedFileKind),
	#[cfg(feature = "go")]
	Go(monochange_go::GoVersionedFileKind),
}

pub(crate) fn versioned_file_kind(
	ecosystem_type: monochange_core::EcosystemType,
	path: &Path,
) -> Option<VersionedFileKind> {
	match ecosystem_type {
		#[cfg(feature = "cargo")]
		monochange_core::EcosystemType::Cargo => {
			monochange_cargo::supported_versioned_file_kind(path).map(VersionedFileKind::Cargo)
		}
		#[cfg(feature = "npm")]
		monochange_core::EcosystemType::Npm => {
			monochange_npm::supported_versioned_file_kind(path).map(VersionedFileKind::Npm)
		}
		#[cfg(feature = "deno")]
		monochange_core::EcosystemType::Deno => {
			monochange_deno::supported_versioned_file_kind(path).map(VersionedFileKind::Deno)
		}
		#[cfg(feature = "dart")]
		monochange_core::EcosystemType::Dart => {
			monochange_dart::supported_versioned_file_kind(path).map(VersionedFileKind::Dart)
		}
		#[cfg(feature = "python")]
		monochange_core::EcosystemType::Python => {
			monochange_python::supported_versioned_file_kind(path).map(VersionedFileKind::Python)
		}
		#[cfg(feature = "go")]
		monochange_core::EcosystemType::Go => {
			monochange_go::supported_versioned_file_kind(path).map(VersionedFileKind::Go)
		}
		_ => None,
	}
}

fn dedup_versioned_file_definitions(
	versioned_files: &[VersionedFileDefinition],
) -> Vec<&VersionedFileDefinition> {
	let mut seen = HashSet::<&VersionedFileDefinition>::new();
	versioned_files
		.iter()
		.filter(|definition| seen.insert(*definition))
		.collect()
}

#[derive(Debug, Default)]
struct VersionedFilePathCache {
	paths_by_pattern: BTreeMap<String, Vec<PathBuf>>,
}

impl VersionedFilePathCache {
	fn prewarm<'a, I>(&mut self, root: &Path, patterns: I) -> MonochangeResult<()>
	where
		I: IntoIterator<Item = &'a str>,
	{
		let pending_patterns = patterns
			.into_iter()
			.filter(|pattern| !self.paths_by_pattern.contains_key(*pattern))
			.collect::<BTreeSet<_>>();
		if pending_patterns.is_empty() {
			return Ok(());
		}

		let resolved_paths = monochange_core::workspace_glob_files_many(root, pending_patterns)?;
		self.paths_by_pattern.extend(resolved_paths);
		Ok(())
	}

	fn paths(&mut self, root: &Path, pattern: &str) -> MonochangeResult<&[PathBuf]> {
		if !self.paths_by_pattern.contains_key(pattern) {
			let resolved_paths = monochange_core::workspace_glob_files(root, pattern)?;
			self.paths_by_pattern
				.insert(pattern.to_string(), resolved_paths);
		}

		Ok(self
			.paths_by_pattern
			.get(pattern)
			.expect("versioned file path cache should include resolved pattern"))
	}
}

fn cached_document_key(path: &Path) -> PathBuf {
	fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn take_cached_document(
	updates: &mut BTreeMap<PathBuf, CachedDocument>,
	path: &Path,
) -> Option<CachedDocument> {
	if let Some(cached) = updates.remove(path) {
		return Some(cached);
	}

	let cache_key = cached_document_key(path);
	let equivalent_path = updates
		.keys()
		.find(|cached_path| cached_document_key(cached_path) == cache_key)
		.cloned();
	equivalent_path.and_then(|path| updates.remove(&path))
}

fn seed_cached_text_updates(
	root: &Path,
	updates: &mut BTreeMap<PathBuf, CachedDocument>,
	base_updates: &[FileUpdate],
) -> MonochangeResult<()> {
	for update in base_updates {
		let contents = String::from_utf8(update.content.clone()).map_err(|error| {
			MonochangeError::Config(format!(
				"failed to parse {} as text: {error}",
				update.path.display()
			))
		})?;
		let path = if update.path.is_absolute() {
			update.path.clone()
		} else {
			root.join(&update.path)
		};
		updates.insert(path, CachedDocument::Text(contents));
	}
	Ok(())
}

pub(crate) fn build_versioned_file_updates_with_base_updates(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
	base_updates: &[FileUpdate],
) -> MonochangeResult<Vec<FileUpdate>> {
	if configuration.packages.is_empty()
		&& configuration.groups.is_empty()
		&& base_updates.is_empty()
	{
		return Ok(Vec::new());
	}

	let released_versions_by_record_id = released_versions_by_record_id(plan);

	let package_by_config_id = packages
		.iter()
		.filter_map(|package| {
			package
				.metadata
				.get("config_id")
				.map(|config_id| (config_id.as_str(), package))
		})
		.collect::<BTreeMap<_, _>>();

	let package_by_native_name = packages
		.iter()
		.map(|package| (package.name.as_str(), package))
		.collect::<BTreeMap<_, _>>();

	let current_versions_by_native_name = packages
		.iter()
		.filter_map(|package| {
			package
				.current_version
				.as_ref()
				.map(|version| (package.name.clone(), version.to_string()))
		})
		.collect::<BTreeMap<_, _>>();

	let released_versions_by_config_id = packages
		.iter()
		.filter_map(|package| {
			package.metadata.get("config_id").and_then(|config_id| {
				released_versions_by_record_id
					.get(&package.id)
					.map(|version| (config_id.as_str(), version.as_str()))
			})
		})
		.collect::<BTreeMap<_, _>>();

	let released_versions_by_native_name = packages
		.iter()
		.filter_map(|package| {
			released_versions_by_record_id
				.get(&package.id)
				.map(|version| (package.name.clone(), version.clone()))
		})
		.collect::<BTreeMap<_, _>>();

	let shared_release_version = shared_release_version(plan);

	let context = VersionedFileUpdateContext {
		package_by_config_id,
		package_by_native_name,
		current_versions_by_native_name,
		released_versions_by_native_name,
		configuration,
	};

	let mut updates = BTreeMap::<PathBuf, CachedDocument>::new();
	seed_cached_text_updates(root, &mut updates, base_updates)?;

	let mut path_cache = VersionedFilePathCache::default();
	path_cache.prewarm(
		root,
		configuration
			.packages
			.iter()
			.filter(|package_definition| {
				released_versions_by_config_id.contains_key(package_definition.id.as_str())
			})
			.flat_map(|package_definition| {
				package_definition
					.versioned_files
					.iter()
					.map(|versioned_file| versioned_file.path.as_str())
			}),
	)?;

	for package_definition in &configuration.packages {
		let Some(version) = released_versions_by_config_id.get(package_definition.id.as_str())
		else {
			continue;
		};

		let matched_package = context
			.package_by_config_id
			.get(package_definition.id.as_str());

		let dep_names = [
			matched_package.map_or(package_definition.id.as_str(), |package| {
				package.name.as_str()
			}),
		];

		for versioned_file in dedup_versioned_file_definitions(&package_definition.versioned_files)
		{
			if let Some(override_name) = versioned_file.name.as_deref() {
				let effective_dep_names = [override_name];
				let resolved_paths = path_cache.paths(root, &versioned_file.path)?;
				apply_versioned_file_definition_to_paths(
					root,
					&mut updates,
					versioned_file,
					resolved_paths,
					version,
					shared_release_version.as_ref(),
					&effective_dep_names,
					&context,
				)?;
				continue;
			}

			let resolved_paths = path_cache.paths(root, &versioned_file.path)?;
			apply_versioned_file_definition_to_paths(
				root,
				&mut updates,
				versioned_file,
				resolved_paths,
				version,
				shared_release_version.as_ref(),
				&dep_names,
				&context,
			)?;
		}
	}

	let planned_group_versions = plan
		.groups
		.iter()
		.filter_map(|group| {
			group
				.planned_version
				.as_ref()
				.map(|version| (group.group_id.as_str(), version))
		})
		.collect::<BTreeMap<_, _>>();
	path_cache.prewarm(
		root,
		configuration
			.groups
			.iter()
			.filter(|group_definition| {
				planned_group_versions.contains_key(group_definition.id.as_str())
			})
			.flat_map(|group_definition| {
				group_definition
					.versioned_files
					.iter()
					.map(|versioned_file| versioned_file.path.as_str())
			}),
	)?;
	let mut group_dep_names = Vec::new();

	for group_definition in &configuration.groups {
		let Some(group_version) = planned_group_versions
			.get(group_definition.id.as_str())
			.map(ToString::to_string)
		else {
			continue;
		};

		group_dep_names.clear();
		group_dep_names.extend(group_definition.packages.iter().map(|member_id| {
			context
				.package_by_config_id
				.get(member_id.as_str())
				.map_or(member_id.as_str(), |package| package.name.as_str())
		}));

		for versioned_file in &group_definition.versioned_files {
			let resolved_paths = path_cache.paths(root, &versioned_file.path)?;
			apply_versioned_file_definition_to_paths(
				root,
				&mut updates,
				versioned_file,
				resolved_paths,
				&group_version,
				Some(&group_version),
				&group_dep_names,
				&context,
			)?;
		}
	}

	apply_inferred_lockfile_updates(
		root,
		configuration,
		packages,
		plan,
		shared_release_version.as_ref(),
		&context,
		&mut updates,
		&mut path_cache,
	)?;

	updates
		.into_iter()
		.map(|(path, document)| serialize_cached_document(&path, document))
		.collect()
}

#[allow(clippy::too_many_arguments)]
fn apply_inferred_lockfile_updates(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	plan: &ReleasePlan,
	shared_release_version: Option<&String>,
	context: &VersionedFileUpdateContext<'_>,
	updates: &mut BTreeMap<PathBuf, CachedDocument>,
	path_cache: &mut VersionedFilePathCache,
) -> MonochangeResult<()> {
	let released_versions = released_versions_by_record_id(plan);
	let mut dep_names_by_lockfile =
		BTreeMap::<PathBuf, (monochange_core::EcosystemType, BTreeSet<&str>)>::new();

	for package in packages
		.iter()
		.filter(|package| released_versions.contains_key(&package.id))
	{
		let Some(ecosystem_type) =
			inferred_lockfile_ecosystem_type(configuration, package.ecosystem)
		else {
			continue;
		};

		for lockfile_path in inferred_lockfile_paths(package) {
			let relative_lockfile = root_relative(root, &lockfile_path);
			let (_, dep_names) = dep_names_by_lockfile
				.entry(relative_lockfile)
				.or_insert_with(|| (ecosystem_type, BTreeSet::new()));
			dep_names.insert(package.name.as_str());
		}
	}

	for (lockfile_path, (ecosystem_type, dep_names)) in dep_names_by_lockfile {
		let definition = VersionedFileDefinition {
			path: lockfile_path.display().to_string(),
			ecosystem_type: Some(ecosystem_type),
			format: None,
			prefix: None,
			fields: None,
			name: None,
			missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
			regex: None,
		};

		// Supported lockfiles can be rewritten directly from the release plan.
		// That keeps normal `monochange release` runs on the fast path instead of paying
		// package-manager startup and dependency-resolution costs for every bump.
		let dep_names = dep_names.into_iter().collect::<Vec<_>>();
		let resolved_paths = path_cache.paths(root, &definition.path)?;
		apply_versioned_file_definition_to_paths(
			root,
			updates,
			&definition,
			resolved_paths,
			"",
			shared_release_version,
			&dep_names,
			context,
		)?;
	}

	Ok(())
}

fn inferred_lockfile_ecosystem_type(
	configuration: &monochange_core::WorkspaceConfiguration,
	ecosystem: Ecosystem,
) -> Option<monochange_core::EcosystemType> {
	match ecosystem {
		#[cfg(feature = "cargo")]
		Ecosystem::Cargo if configuration.cargo.lockfile_commands.is_empty() => {
			Some(monochange_core::EcosystemType::Cargo)
		}
		#[cfg(feature = "npm")]
		Ecosystem::Npm if configuration.npm.lockfile_commands.is_empty() => {
			Some(monochange_core::EcosystemType::Npm)
		}
		#[cfg(feature = "deno")]
		Ecosystem::Deno if configuration.deno.lockfile_commands.is_empty() => {
			Some(monochange_core::EcosystemType::Deno)
		}
		#[cfg(feature = "dart")]
		Ecosystem::Dart if configuration.dart.lockfile_commands.is_empty() => {
			Some(monochange_core::EcosystemType::Dart)
		}
		#[cfg(feature = "python")]
		Ecosystem::Python if configuration.python.lockfile_commands.is_empty() => {
			Some(monochange_core::EcosystemType::Python)
		}
		#[cfg(feature = "go")]
		Ecosystem::Go if configuration.go.lockfile_commands.is_empty() => {
			Some(monochange_core::EcosystemType::Go)
		}
		_ => None,
	}
}

fn inferred_lockfile_paths(package: &PackageRecord) -> Vec<PathBuf> {
	match package.ecosystem {
		#[cfg(feature = "cargo")]
		Ecosystem::Cargo => monochange_cargo::discover_lockfiles(package),
		#[cfg(feature = "npm")]
		Ecosystem::Npm => monochange_npm::discover_lockfiles(package),
		#[cfg(feature = "deno")]
		Ecosystem::Deno => monochange_deno::discover_lockfiles(package),
		#[cfg(feature = "dart")]
		Ecosystem::Dart => monochange_dart::discover_lockfiles(package),
		#[cfg(feature = "go")]
		Ecosystem::Go => monochange_go::discover_lockfiles(package),
		_ => Vec::new(),
	}
}

fn render_cached_document_bytes(
	_path: &Path,
	document: CachedDocument,
) -> MonochangeResult<Vec<u8>> {
	match document {
		CachedDocument::Json(value) => {
			let mut rendered = Vec::new();
			serde_json::to_writer_pretty(&mut rendered, &value)
				.map_err(|error| MonochangeError::Config(error.to_string()))?;
			rendered.push(b'\n');
			Ok(rendered)
		}

		CachedDocument::Yaml(mapping) => {
			serde_yaml_ng::to_string(&mapping)
				.map(String::into_bytes)
				.map_err(|error| MonochangeError::Config(error.to_string()))
		}

		CachedDocument::Text(contents) => Ok(contents.into_bytes()),

		CachedDocument::Bytes(contents) => Ok(contents),
	}
}

pub(crate) fn render_cached_document_text(
	path: &Path,
	document: CachedDocument,
) -> MonochangeResult<String> {
	String::from_utf8(render_cached_document_bytes(path, document)?).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse {} as text: {error}",
			path.display()
		))
	})
}

pub(crate) fn serialize_cached_document(
	path: &Path,
	document: CachedDocument,
) -> MonochangeResult<FileUpdate> {
	Ok(FileUpdate {
		path: path.to_path_buf(),
		content: render_cached_document_bytes(path, document)?,
	})
}

pub(crate) fn read_cached_text_document(
	updates: &mut BTreeMap<PathBuf, CachedDocument>,
	path: &Path,
) -> MonochangeResult<String> {
	if let Some(cached) = take_cached_document(updates, path) {
		return render_cached_document_text(path, cached);
	}
	let contents = fs::read(path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
	})?;
	String::from_utf8(contents).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse {} as text: {error}",
			path.display()
		))
	})
}

pub(crate) fn read_cached_document(
	updates: &mut BTreeMap<PathBuf, CachedDocument>,
	path: &Path,
	ecosystem_type: monochange_core::EcosystemType,
) -> MonochangeResult<CachedDocument> {
	if let Some(cached) = take_cached_document(updates, path) {
		return Ok(cached);
	}

	let Some(kind) = versioned_file_kind(ecosystem_type, path) else {
		return Err(MonochangeError::Config(format!(
			"unsupported versioned file `{}` for ecosystem `{}`",
			path.display(),
			match ecosystem_type {
				monochange_core::EcosystemType::Cargo => "cargo",
				monochange_core::EcosystemType::Npm => "npm",
				monochange_core::EcosystemType::Deno => "deno",
				monochange_core::EcosystemType::Dart => "dart",
				monochange_core::EcosystemType::Python => "python",
				monochange_core::EcosystemType::Go => "go",
				_ => "unknown",
			},
		)));
	};

	let contents = fs::read(path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
	})?;

	#[cfg(feature = "npm")]
	if matches!(
		kind,
		VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::BunLockBinary)
	) {
		return Ok(CachedDocument::Bytes(contents));
	}

	let text_contents = String::from_utf8(contents.clone())
		.map_err(|error| {
			MonochangeError::Config(format!(
				"failed to parse {} as text: {error}",
				path.display()
			))
		})
		.ok();

	match kind {
		#[cfg(feature = "cargo")]
		VersionedFileKind::Cargo(kind) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			monochange_cargo::update_versioned_file_text(
				&contents,
				kind,
				&[],
				None,
				None,
				&BTreeMap::new(),
				&BTreeMap::new(),
			)
			.map_err(|error| {
				MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
			})?;
			Ok(CachedDocument::Text(contents))
		}
		#[cfg(feature = "npm")]
		VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::PnpmLock) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			monochange_npm::update_pnpm_lock_text(&contents, &BTreeMap::new()).map_err(
				|error| {
					MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
				},
			)?;
			Ok(CachedDocument::Text(contents))
		}
		#[cfg(feature = "dart")]
		VersionedFileKind::Dart(monochange_dart::DartVersionedFileKind::Lock) => {
			let Some(contents) = text_contents.as_ref() else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			let mapping =
				serde_yaml_ng::from_str::<serde_yaml_ng::Mapping>(contents).map_err(|error| {
					MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
				})?;
			Ok(CachedDocument::Yaml(mapping))
		}
		#[cfg(feature = "npm")]
		VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::BunLock) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			Ok(CachedDocument::Text(contents))
		}
		#[cfg(feature = "npm")]
		VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::BunLockBinary) => {
			Ok(CachedDocument::Bytes(contents))
		}
		#[cfg(feature = "npm")]
		VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::Manifest) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			monochange_core::update_json_manifest_text(&contents, None, &[], &BTreeMap::new())
				.map_err(|error| {
					MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
				})?;
			Ok(CachedDocument::Text(contents))
		}
		#[cfg(feature = "deno")]
		VersionedFileKind::Deno(monochange_deno::DenoVersionedFileKind::Manifest) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			monochange_core::update_json_manifest_text(&contents, None, &[], &BTreeMap::new())
				.map_err(|error| {
					MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
				})?;
			Ok(CachedDocument::Text(contents))
		}
		#[cfg(feature = "python")]
		VersionedFileKind::Python(monochange_python::PythonVersionedFileKind::Manifest) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"{} is not valid UTF-8",
					path.display()
				)));
			};
			let contents = monochange_python::update_versioned_file_text(
				&contents,
				monochange_python::PythonVersionedFileKind::Manifest,
				None,
				&BTreeMap::new(),
			)
			.map_err(|error| {
				MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
			})?;
			Ok(CachedDocument::Text(contents))
		}
		#[cfg(feature = "python")]
		VersionedFileKind::Python(monochange_python::PythonVersionedFileKind::Lock) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"{} is not valid UTF-8",
					path.display()
				)));
			};
			Ok(CachedDocument::Text(contents))
		}
		#[cfg(feature = "go")]
		VersionedFileKind::Go(_) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			Ok(CachedDocument::Text(contents))
		}
		#[cfg(feature = "dart")]
		VersionedFileKind::Dart(monochange_dart::DartVersionedFileKind::Manifest) => {
			let Some(contents) = text_contents else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			monochange_dart::update_manifest_text(&contents, None, &[], &BTreeMap::new()).map_err(
				|error| {
					MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
				},
			)?;
			Ok(CachedDocument::Text(contents))
		}
		#[cfg(feature = "npm")]
		VersionedFileKind::Npm(_) => {
			let Some(contents) = text_contents.as_ref() else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			let value = serde_json::from_str::<serde_json::Value>(contents).map_err(|error| {
				MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
			})?;
			Ok(CachedDocument::Json(value))
		}
		#[cfg(feature = "deno")]
		VersionedFileKind::Deno(_) => {
			let Some(contents) = text_contents.as_ref() else {
				return Err(MonochangeError::Config(format!(
					"failed to parse {} as text",
					path.display()
				)));
			};
			let value = serde_json::from_str::<serde_json::Value>(contents).map_err(|error| {
				MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
			})?;
			Ok(CachedDocument::Json(value))
		}
	}
}

// patch-coverage:ignore-start -- fallback prefixes mirror ecosystem defaults covered in ecosystem crates.
pub(crate) fn resolve_versioned_prefix(
	definition: &VersionedFileDefinition,
	context: &VersionedFileUpdateContext<'_>,
) -> String {
	if let Some(prefix) = &definition.prefix {
		return prefix.clone();
	}
	let ecosystem_type = definition
		.ecosystem_type
		.expect("typed versioned_files should always have an ecosystem type");
	let ecosystem_prefix = match ecosystem_type {
		monochange_core::EcosystemType::Cargo => {
			context
				.configuration
				.cargo
				.dependency_version_prefix
				.clone()
		}
		monochange_core::EcosystemType::Npm => {
			context.configuration.npm.dependency_version_prefix.clone()
		}
		monochange_core::EcosystemType::Deno => {
			context.configuration.deno.dependency_version_prefix.clone()
		}
		monochange_core::EcosystemType::Dart => {
			context.configuration.dart.dependency_version_prefix.clone()
		}
		monochange_core::EcosystemType::Python => {
			context
				.configuration
				.python
				.dependency_version_prefix
				.clone()
		}
		monochange_core::EcosystemType::Go => {
			context.configuration.go.dependency_version_prefix.clone()
		}
		_ => None,
	};
	ecosystem_prefix.unwrap_or_else(|| {
		match ecosystem_type {
			monochange_core::EcosystemType::Cargo => {
				monochange_cargo::default_dependency_version_prefix().to_string()
			}
			monochange_core::EcosystemType::Npm => {
				monochange_npm::default_dependency_version_prefix().to_string()
			}
			monochange_core::EcosystemType::Deno => {
				monochange_deno::default_dependency_version_prefix().to_string()
			}
			monochange_core::EcosystemType::Dart => {
				monochange_dart::default_dependency_version_prefix().to_string()
			}
			monochange_core::EcosystemType::Python => {
				monochange_python::default_dependency_version_prefix().to_string()
			}
			monochange_core::EcosystemType::Go => {
				monochange_go::default_dependency_version_prefix().to_string()
			}
			_ => String::new(),
		}
	})
}
// patch-coverage:ignore-end

// patch-coverage:ignore-start -- fallback fields mirror ecosystem defaults covered in ecosystem crates.
pub(crate) fn expand_versioned_file_fields<N: AsRef<str>>(
	definition: &VersionedFileDefinition,
	dep_names: &[N],
) -> Vec<String> {
	let ecosystem_type = definition
		.ecosystem_type
		.expect("typed versioned_files should always have an ecosystem type");
	let field_templates = definition.fields.as_ref().map_or_else(
		|| {
			let default_fields: &[&str] = match ecosystem_type {
				monochange_core::EcosystemType::Cargo => {
					monochange_cargo::default_dependency_fields()
				}
				monochange_core::EcosystemType::Npm => monochange_npm::default_dependency_fields(),
				monochange_core::EcosystemType::Deno => {
					monochange_deno::default_dependency_fields()
				}
				monochange_core::EcosystemType::Dart => {
					monochange_dart::default_dependency_fields()
				}
				monochange_core::EcosystemType::Python => {
					monochange_python::default_dependency_fields()
				}
				monochange_core::EcosystemType::Go => monochange_go::default_dependency_fields(),
				_ => &[],
			};
			default_fields
				.iter()
				.map(ToString::to_string)
				.collect::<Vec<_>>()
		},
		Clone::clone,
	);
	let mut fields = Vec::new();
	for field_template in field_templates {
		if field_template.contains("{{ name }}") {
			fields.extend(
				dep_names
					.iter()
					.map(|name| field_template.replace("{{ name }}", name.as_ref())),
			);
			continue;
		}
		if field_template.contains("{{name}}") {
			fields.extend(
				dep_names
					.iter()
					.map(|name| field_template.replace("{{name}}", name.as_ref())),
			);
			continue;
		}
		fields.push(field_template);
	}
	fields
}
// patch-coverage:ignore-end

fn update_json_field_path(
	value: &mut serde_json::Value,
	path: &str,
	version: &str,
) -> MonochangeResult<()> {
	let segments = field_path_segments(path)?;
	let (leaf, parent_segments) = segments
		.split_last()
		.expect("validated field paths contain at least one segment");
	let leaf = *leaf;
	let mut cursor = value;
	for segment in parent_segments {
		let object = cursor.as_object_mut().ok_or_else(|| {
			MonochangeError::Config(format!(
				"versioned_files field `{path}` cannot traverse non-object segment `{segment}`"
			))
		})?;
		cursor = object.get_mut(*segment).ok_or_else(|| {
			MonochangeError::Config(format!(
				"versioned_files field `{path}` is missing segment `{segment}`"
			))
		})?;
	}

	let object = cursor.as_object_mut().ok_or_else(|| {
		MonochangeError::Config(format!(
			"versioned_files field `{path}` cannot set `{leaf}` on a non-object value"
		))
	})?;
	if !object.contains_key(leaf) {
		return Err(MonochangeError::Config(format!(
			"versioned_files field `{path}` is missing leaf `{leaf}`"
		)));
	}
	object.insert(
		leaf.to_string(),
		serde_json::Value::String(version.to_string()),
	);
	Ok(())
}

fn add_json_field_path(
	value: &mut serde_json::Value,
	path: &str,
	version: &str,
) -> MonochangeResult<()> {
	let segments = field_path_segments(path)?;
	let (leaf, parent_segments) = segments
		.split_last()
		.expect("validated field paths contain at least one segment");
	let mut cursor = value;

	for segment in parent_segments {
		let object = cursor.as_object_mut().ok_or_else(|| {
			MonochangeError::Config(format!(
				"versioned_files field `{path}` cannot traverse non-object segment `{segment}`"
			))
		})?;
		cursor = object
			.entry((*segment).to_string())
			.or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
	}

	let object = cursor.as_object_mut().ok_or_else(|| {
		MonochangeError::Config(format!(
			"versioned_files field `{path}` cannot set `{leaf}` on a non-object value"
		))
	})?;
	object.insert(
		(*leaf).to_string(),
		serde_json::Value::String(version.to_string()),
	);
	Ok(())
}

fn update_toml_field_path(
	document: &mut toml_edit::DocumentMut,
	path: &str,
	version: &str,
) -> MonochangeResult<()> {
	let segments = field_path_segments(path)?;
	let (leaf, parent_segments) = segments
		.split_last()
		.expect("validated field paths contain at least one segment");
	let leaf = *leaf;
	let mut table: &mut dyn toml_edit::TableLike = document.as_table_mut();
	for segment in parent_segments {
		let item = table.get_mut(segment).ok_or_else(|| {
			MonochangeError::Config(format!(
				"versioned_files field `{path}` is missing segment `{segment}`"
			))
		})?;
		table = item.as_table_like_mut().ok_or_else(|| {
			MonochangeError::Config(format!(
				"versioned_files field `{path}` cannot traverse non-table segment `{segment}`"
			))
		})?;
	}

	if !table.contains_key(leaf) {
		return Err(MonochangeError::Config(format!(
			"versioned_files field `{path}` is missing leaf `{leaf}`"
		)));
	}
	table.insert(leaf, toml_edit::value(version));
	Ok(())
}

fn add_toml_field_path(
	document: &mut toml_edit::DocumentMut,
	path: &str,
	version: &str,
) -> MonochangeResult<()> {
	let segments = field_path_segments(path)?;
	let (leaf, parent_segments) = segments
		.split_last()
		.expect("validated field paths contain at least one segment");
	let mut table: &mut dyn toml_edit::TableLike = document.as_table_mut();

	for segment in parent_segments {
		if !table.contains_key(segment) {
			table.insert(segment, toml_edit::Item::Table(toml_edit::Table::new()));
		}
		// patch-coverage:ignore-start -- contains_key/insert above guarantees the segment exists before lookup.
		let item = table.get_mut(segment).ok_or_else(|| {
			MonochangeError::Config(format!(
				"versioned_files field `{path}` is missing segment `{segment}`"
			))
		})?;
		// patch-coverage:ignore-end
		table = item.as_table_like_mut().ok_or_else(|| {
			MonochangeError::Config(format!(
				"versioned_files field `{path}` cannot traverse non-table segment `{segment}`"
			))
		})?;
	}

	table.insert(leaf, toml_edit::value(version));
	Ok(())
}

fn update_yaml_field_path(
	value: &mut serde_yaml_ng::Value,
	path: &str,
	version: &str,
) -> MonochangeResult<()> {
	let segments = field_path_segments(path)?;
	let (leaf, parent_segments) = segments
		.split_last()
		.expect("validated field paths contain at least one segment");
	let leaf = *leaf;
	let mut cursor = value;
	for segment in parent_segments {
		let mapping = cursor.as_mapping_mut().ok_or_else(|| {
			MonochangeError::Config(format!(
				"versioned_files field `{path}` cannot traverse non-mapping segment `{segment}`"
			))
		})?;
		let key = serde_yaml_ng::Value::String((*segment).to_string());
		cursor = mapping.get_mut(&key).ok_or_else(|| {
			MonochangeError::Config(format!(
				"versioned_files field `{path}` is missing segment `{segment}`"
			))
		})?;
	}

	let mapping = cursor.as_mapping_mut().ok_or_else(|| {
		MonochangeError::Config(format!(
			"versioned_files field `{path}` cannot set `{leaf}` on a non-mapping value"
		))
	})?;
	let key = serde_yaml_ng::Value::String(leaf.to_string());
	if !mapping.contains_key(&key) {
		return Err(MonochangeError::Config(format!(
			"versioned_files field `{path}` is missing leaf `{leaf}`"
		)));
	}
	mapping.insert(key, serde_yaml_ng::Value::String(version.to_string()));
	Ok(())
}

fn add_yaml_field_path(
	value: &mut serde_yaml_ng::Value,
	path: &str,
	version: &str,
) -> MonochangeResult<()> {
	let segments = field_path_segments(path)?;
	let (leaf, parent_segments) = segments
		.split_last()
		.expect("validated field paths contain at least one segment");
	let mut cursor = value;

	for segment in parent_segments {
		let mapping = cursor.as_mapping_mut().ok_or_else(|| {
			MonochangeError::Config(format!(
				"versioned_files field `{path}` cannot traverse non-mapping segment `{segment}`"
			))
		})?;
		let key = serde_yaml_ng::Value::String((*segment).to_string());
		cursor = mapping
			.entry(key)
			.or_insert_with(|| serde_yaml_ng::Value::Mapping(serde_yaml_ng::Mapping::default()));
	}

	let mapping = cursor.as_mapping_mut().ok_or_else(|| {
		MonochangeError::Config(format!(
			"versioned_files field `{path}` cannot set `{leaf}` on a non-mapping value"
		))
	})?;
	mapping.insert(
		serde_yaml_ng::Value::String((*leaf).to_string()),
		serde_yaml_ng::Value::String(version.to_string()),
	);
	Ok(())
}

fn field_path_segments(path: &str) -> MonochangeResult<Vec<&str>> {
	let segments = path.split('.').map(str::trim).collect::<Vec<_>>();
	if segments.is_empty() || segments.iter().any(|segment| segment.is_empty()) {
		return Err(MonochangeError::Config(format!(
			"versioned_files field path `{path}` must contain non-empty dot-separated segments"
		)));
	}
	Ok(segments)
}

fn render_versioned_template(template: &str, name: Option<&str>, version: &str) -> String {
	template
		.replace("{{ version }}", version)
		.replace("{{version}}", version)
		.replace("{{ name }}", name.unwrap_or_default())
		.replace("{{name}}", name.unwrap_or_default())
}

fn update_env_key(contents: &str, key: &str, version: &str) -> MonochangeResult<String> {
	let mut found = false;
	let mut output = String::with_capacity(contents.len());
	for line in contents.split_inclusive('\n') {
		let (body, ending) = line
			.strip_suffix('\n')
			.map_or((line, ""), |body| (body, "\n"));
		let trimmed_start = body.trim_start();
		let export_prefix = "export ";
		let assignment = trimmed_start
			.strip_prefix(export_prefix)
			.unwrap_or(trimmed_start);
		let Some((candidate, _old_value)) = assignment.split_once('=') else {
			output.push_str(body);
			output.push_str(ending);
			continue;
		};

		if candidate.trim() != key {
			output.push_str(body);
			output.push_str(ending);
			continue;
		}

		found = true;
		let leading_len = body.len() - trimmed_start.len();
		output.push_str(&body[..leading_len]);
		if trimmed_start.starts_with(export_prefix) {
			output.push_str(export_prefix);
		}
		output.push_str(candidate);
		output.push('=');
		output.push_str(version);
		output.push_str(ending);
	}

	if !found {
		return Err(MonochangeError::Config(format!(
			"versioned_files env field `{key}` was not found"
		)));
	}
	Ok(output)
}

fn add_env_key(contents: &str, key: &str, version: &str) -> String {
	let mut output = String::with_capacity(contents.len() + key.len() + version.len() + 2);
	output.push_str(contents);
	if !output.is_empty() && !output.ends_with('\n') {
		output.push('\n');
	}
	output.push_str(key);
	output.push('=');
	output.push_str(version);
	output.push('\n');
	output
}

fn is_missing_field_error(error: &MonochangeError) -> bool {
	let message = error.to_string();
	message.contains(" is missing leaf ")
		|| message.contains(" is missing segment ")
		|| message.contains(" was not found")
}

fn update_format_error_action(
	field_path: &str,
	error: MonochangeError,
	missing_field_behavior: monochange_core::MissingFieldBehavior,
) -> MonochangeResult<bool> {
	if !is_missing_field_error(&error) {
		return Err(error);
	}

	match missing_field_behavior {
		monochange_core::MissingFieldBehavior::Ignore => Ok(false),
		monochange_core::MissingFieldBehavior::Add => {
			let _ = field_path;
			Ok(true)
		}
	}
}

fn update_format_versioned_file_text(
	contents: &str,
	format: monochange_core::VersionedFileFormat,
	fields: &[String],
	version: &str,
	name: Option<&str>,
	missing_field_behavior: monochange_core::MissingFieldBehavior,
) -> MonochangeResult<String> {
	match format {
		monochange_core::VersionedFileFormat::Json => {
			let mut value =
				serde_json::from_str::<serde_json::Value>(contents).map_err(|error| {
					MonochangeError::Config(format!("failed to parse json versioned file: {error}"))
				})?;
			for field in fields {
				let rendered_field = render_versioned_template(field, name, version);
				if let Err(error) = update_json_field_path(&mut value, &rendered_field, version) {
					let should_add =
						update_format_error_action(&rendered_field, error, missing_field_behavior)?;
					if should_add {
						add_json_field_path(&mut value, &rendered_field, version)?;
					}
				}
			}
			let mut output = serde_json::to_string_pretty(&value).unwrap_or_else(|error| {
				// patch-coverage:ignore-start -- serde_json::Value serialization is infallible for supported values.
				panic!("serializing serde_json::Value should not fail: {error}")
				// patch-coverage:ignore-end
			});
			output.push('\n');
			Ok(output)
		}
		monochange_core::VersionedFileFormat::Toml => {
			let mut document = contents
				.parse::<toml_edit::DocumentMut>()
				.map_err(|error| {
					MonochangeError::Config(format!("failed to parse toml versioned file: {error}"))
				})?;
			for field in fields {
				let rendered_field = render_versioned_template(field, name, version);
				if let Err(error) = update_toml_field_path(&mut document, &rendered_field, version)
				{
					let should_add =
						update_format_error_action(&rendered_field, error, missing_field_behavior)?;
					if should_add {
						add_toml_field_path(&mut document, &rendered_field, version)?;
					}
				}
			}
			Ok(document.to_string())
		}
		monochange_core::VersionedFileFormat::Yaml | monochange_core::VersionedFileFormat::Yml => {
			let mut value =
				serde_yaml_ng::from_str::<serde_yaml_ng::Value>(contents).map_err(|error| {
					MonochangeError::Config(format!("failed to parse yaml versioned file: {error}"))
				})?;
			for field in fields {
				let rendered_field = render_versioned_template(field, name, version);
				if let Err(error) = update_yaml_field_path(&mut value, &rendered_field, version) {
					let should_add =
						update_format_error_action(&rendered_field, error, missing_field_behavior)?;
					if should_add {
						add_yaml_field_path(&mut value, &rendered_field, version)?;
					}
				}
			}
			Ok(serde_yaml_ng::to_string(&value).unwrap_or_else(|error| {
				// patch-coverage:ignore-start -- serde_yaml_ng::Value serialization is infallible for supported values.
				panic!("serializing serde_yaml_ng::Value should not fail: {error}")
				// patch-coverage:ignore-end
			}))
		}
		monochange_core::VersionedFileFormat::Env => {
			let mut output = contents.to_string();
			for field in fields {
				let key = render_versioned_template(field, name, version);
				match update_env_key(&output, &key, version) {
					Ok(updated) => output = updated,
					Err(error) => {
						if update_format_error_action(&key, error, missing_field_behavior)? {
							output = add_env_key(&output, &key, version);
						}
					}
				}
			}
			Ok(output)
		}
	}
}

fn update_versioned_file_regex(
	contents: &str,
	pattern: &str,
	version: &str,
) -> MonochangeResult<String> {
	let regex = Regex::new(pattern).map_err(|error| {
		MonochangeError::Config(format!(
			"invalid versioned_files regex `{pattern}`: {error}"
		))
	})?;
	Ok(regex
		.replace_all(contents, |captures: &regex::Captures<'_>| {
			let whole_match = captures
				.get(0)
				.expect("regex replacement should always receive the full match");
			let version_match = captures
				.name("version")
				.expect("validated versioned_files regex should always capture `version`");
			let prefix = &whole_match.as_str()[..version_match.start() - whole_match.start()];
			let suffix = &whole_match.as_str()[version_match.end() - whole_match.start()..];
			format!("{prefix}{version}{suffix}")
		})
		.into_owned())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_versioned_file_definition_to_paths(
	root: &Path,
	updates: &mut BTreeMap<PathBuf, CachedDocument>,
	definition: &VersionedFileDefinition,
	resolved_paths: &[PathBuf],
	owner_version: &str,
	shared_release_version: Option<&String>,
	dep_names: &[impl AsRef<str>],
	context: &VersionedFileUpdateContext<'_>,
) -> MonochangeResult<()> {
	if let Some(pattern) = &definition.regex {
		for resolved_path in resolved_paths {
			let resolved_path = resolved_path.clone();
			let contents = read_cached_text_document(updates, &resolved_path)?;
			updates.insert(
				resolved_path,
				CachedDocument::Text(update_versioned_file_regex(
					&contents,
					pattern,
					owner_version,
				)?),
			);
		}
		return Ok(());
	}

	if let Some(format) = definition.format {
		let fields = definition.fields.as_ref().ok_or_else(|| {
			MonochangeError::Config(format!(
				"versioned file `{}` with format mode is missing fields",
				definition.path
			))
		})?;
		let name = dep_names.first().map(AsRef::as_ref);
		for resolved_path in resolved_paths {
			let resolved_path = resolved_path.clone();
			let contents = read_cached_text_document(updates, &resolved_path)?;
			let changed_text = update_format_versioned_file_text(
				&contents,
				format,
				fields,
				owner_version,
				name,
				definition.missing_field_behavior,
			)?;
			updates.insert(resolved_path, CachedDocument::Text(changed_text));
		}
		return Ok(());
	}

	let ecosystem_type = definition.ecosystem_type.ok_or_else(|| {
		MonochangeError::Config(format!(
			"versioned file `{}` is missing an ecosystem type",
			definition.path
		))
	})?;
	let prefix = resolve_versioned_prefix(definition, context);
	let expanded_fields = expand_versioned_file_fields(definition, dep_names);
	let fields = expanded_fields
		.iter()
		.map(String::as_str)
		.collect::<Vec<_>>();
	// Only update the version field if explicitly requested in fields.
	// The version field should never be updated by versioned_files unless
	// the user explicitly specifies it in the fields configuration.
	// Only update the version field if 'version' is explicitly in the fields configuration.
	// This applies to ALL versioned files (both package-level and group-level).
	let update_version = fields.contains(&"version");
	let effective_owner_version = if update_version {
		Some(owner_version)
	} else {
		None
	};
	let versioned_deps: BTreeMap<String, String> = dep_names
		.iter()
		.filter_map(|name| {
			let name = name.as_ref();
			context
				.released_versions_by_native_name
				.get(name)
				.map(|version| (name.to_string(), format!("{prefix}{version}")))
		})
		.collect();
	let raw_versions: BTreeMap<String, String> = dep_names
		.iter()
		.filter_map(|name| {
			let name = name.as_ref();
			context
				.released_versions_by_native_name
				.get(name)
				.map(|version| (name.to_string(), version.clone()))
		})
		.collect();
	if versioned_deps.is_empty() && raw_versions.is_empty() {
		return Ok(());
	}

	for resolved_path in resolved_paths {
		let resolved_path = resolved_path.clone();
		let Some(kind) = versioned_file_kind(ecosystem_type, &resolved_path) else {
			return Err(MonochangeError::Config(format!(
				"versioned_files glob `{}` matched unsupported file `{}` for ecosystem `{}`; narrow the glob or change the `type`",
				definition.path,
				resolved_path.display(),
				match ecosystem_type {
					monochange_core::EcosystemType::Cargo => "cargo",
					monochange_core::EcosystemType::Npm => "npm",
					monochange_core::EcosystemType::Deno => "deno",
					monochange_core::EcosystemType::Dart => "dart",
					monochange_core::EcosystemType::Python => "python",
					monochange_core::EcosystemType::Go => "go",
					_ => "unknown",
				},
			)));
		};
		let package_paths_by_name = dep_names
			.iter()
			.filter_map(|name| {
				let name = name.as_ref();
				context.package_by_native_name.get(name).map(|package| {
					(
						name.to_string(),
						relative_to_root(
							resolved_path.parent().unwrap_or(root),
							package
								.manifest_path
								.parent()
								.unwrap_or(&package.workspace_root),
						)
						.unwrap_or_else(|| {
							package
								.manifest_path
								.parent()
								.unwrap_or(&package.workspace_root)
								.to_path_buf()
						}),
					)
				})
			})
			.collect::<BTreeMap<_, _>>();
		let mut document = read_cached_document(updates, &resolved_path, ecosystem_type)?;
		match (&mut document, kind) {
			#[cfg(feature = "cargo")]
			(CachedDocument::Text(contents), VersionedFileKind::Cargo(kind)) => {
				*contents = monochange_cargo::update_versioned_file_text(
					contents,
					kind,
					&fields,
					effective_owner_version,
					shared_release_version.map(String::as_str),
					&versioned_deps,
					&raw_versions,
				)
				.map_err(|error| {
					MonochangeError::Config(format!(
						"failed to parse {}: {error}",
						resolved_path.display()
					))
				})?;
			}
			#[cfg(feature = "npm")]
			(CachedDocument::Text(contents), VersionedFileKind::Npm(kind)) => {
				if kind == monochange_npm::NpmVersionedFileKind::Manifest {
					*contents = monochange_core::update_json_manifest_text(
						contents,
						shared_release_version
							.map(String::as_str)
							.or(Some(owner_version)),
						&fields,
						&versioned_deps,
					)
					.map_err(|error| {
						MonochangeError::Config(format!(
							"failed to parse {}: {error}",
							resolved_path.display()
						))
					})?;
				} else if kind == monochange_npm::NpmVersionedFileKind::BunLock {
					*contents = monochange_npm::update_bun_lock(contents, &raw_versions);
				} else if kind == monochange_npm::NpmVersionedFileKind::PnpmLock {
					*contents = monochange_npm::update_pnpm_lock_text(contents, &raw_versions)
						.map_err(|error| {
							MonochangeError::Config(format!(
								"failed to parse {}: {error}",
								resolved_path.display()
							))
						})?;
				}
			}
			#[cfg(feature = "npm")]
			(
				CachedDocument::Json(value),
				VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::PackageLock),
			) => {
				monochange_npm::update_package_lock(value, &package_paths_by_name, &raw_versions);
			}
			#[cfg(feature = "npm")]
			(
				CachedDocument::Bytes(contents),
				VersionedFileKind::Npm(monochange_npm::NpmVersionedFileKind::BunLockBinary),
			) => {
				let old_versions = dep_names
					.iter()
					.filter_map(|name| {
						let name = name.as_ref();
						context
							.current_versions_by_native_name
							.get(name)
							.map(|version| (name.to_string(), version.clone()))
					})
					.collect::<BTreeMap<_, _>>();
				*contents =
					monochange_npm::update_bun_lock_binary(contents, &old_versions, &raw_versions);
			}
			#[cfg(feature = "deno")]
			(
				CachedDocument::Text(contents),
				VersionedFileKind::Deno(monochange_deno::DenoVersionedFileKind::Manifest),
			) => {
				*contents = monochange_core::update_json_manifest_text(
					contents,
					effective_owner_version,
					&fields,
					&versioned_deps,
				)
				.map_err(|error| {
					MonochangeError::Config(format!(
						"failed to parse {}: {error}",
						resolved_path.display()
					))
				})?;
			}
			#[cfg(feature = "deno")]
			(
				CachedDocument::Json(value),
				VersionedFileKind::Deno(monochange_deno::DenoVersionedFileKind::Lock),
			) => {
				monochange_deno::update_lockfile(value, &raw_versions);
			}
			#[cfg(feature = "dart")]
			(
				CachedDocument::Text(contents),
				VersionedFileKind::Dart(monochange_dart::DartVersionedFileKind::Manifest),
			) => {
				*contents = monochange_dart::update_manifest_text(
					contents,
					effective_owner_version,
					&fields,
					&versioned_deps,
				)
				.map_err(|error| {
					MonochangeError::Config(format!(
						"failed to parse {}: {error}",
						resolved_path.display()
					))
				})?;
			}
			#[cfg(feature = "dart")]
			(
				CachedDocument::Yaml(mapping),
				VersionedFileKind::Dart(monochange_dart::DartVersionedFileKind::Lock),
			) => {
				monochange_dart::update_pubspec_lock(mapping, &raw_versions);
			}
			#[cfg(feature = "go")]
			(
				CachedDocument::Text(contents),
				VersionedFileKind::Go(monochange_go::GoVersionedFileKind::GoMod),
			) => {
				*contents = monochange_go::update_go_mod_text(contents, &versioned_deps);
			}
			#[cfg(feature = "python")]
			(CachedDocument::Text(contents), VersionedFileKind::Python(kind)) => {
				*contents = monochange_python::update_versioned_file_text(
					contents,
					kind,
					effective_owner_version,
					&versioned_deps,
				)
				.map_err(|error| {
					MonochangeError::Config(format!(
						"failed to parse {}: {error}",
						resolved_path.display()
					))
				})?;
			}
			_ => {}
		}
		updates.insert(resolved_path, document);
	}
	Ok(())
}

pub(crate) fn released_versions_by_record_id(plan: &ReleasePlan) -> BTreeMap<String, String> {
	let mut released_versions = plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
		.filter_map(|decision| {
			decision
				.planned_version
				.as_ref()
				.map(|version| (decision.package_id.clone(), version.to_string()))
		})
		.collect::<BTreeMap<_, _>>();

	for group in &plan.groups {
		let Some(version) = group.planned_version.as_ref() else {
			continue;
		};
		for member in &group.members {
			released_versions.insert(member.clone(), version.to_string());
		}
	}

	released_versions
}

#[cfg(test)]
#[path = "__tests__/versioned_files_tests.rs"]
mod tests;
