//! Resolution of declared release values at prepare time.
//!
//! Values come from three sources, and the source decides which ordering
//! guarantees hold:
//!
//! - **counters** are read from a user-created file and optionally stamped
//!   (`increment`, `add`, or `none`), which is the only ordering guarantee
//!   available;
//! - **derived** values are computed from a file hash, the environment, git, or
//!   a timestamp, and carry no ordering guarantee at all;
//! - **context** values (calendar parts and ordinals) are computed by the
//!   caller and live in [`monochange_core::versioning::LabelInputs`].
//!
//! Stamping is idempotent per release: a release whose record already exists
//! reuses the values frozen in that record instead of advancing counters.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageDefinition;
use monochange_core::versioning::CounterWriteBack;
use monochange_core::versioning::HashAlgorithm;
use monochange_core::versioning::LabelInputs;
use monochange_core::versioning::ReleaseTimestamp;
use monochange_core::versioning::ResetPolicy;
use monochange_core::versioning::ResolvedValue;
use monochange_core::versioning::ValueDefinition;
use monochange_core::versioning::VersionSchemeDefinition;
use monochange_core::versioning::chain_label_inputs;
use monochange_core::versioning::counter_from_json;
use monochange_core::versioning::encode_hash;
use monochange_core::versioning::render_version_template;

/// Values resolved for one package during prepare.
#[derive(Debug, Clone, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PackageValues {
	/// Rendered value strings keyed by declared value id.
	pub values: BTreeMap<String, String>,
	/// Rendered display label, when the package declares a scheme.
	pub label: Option<String>,
	/// Counter files that must be rewritten after stamping.
	pub write_backs: Vec<CounterWriteBack>,
	/// Whether every declared value carries an ordering guarantee.
	pub monotonic: bool,
}

/// Everything resolved for a release.
#[derive(Debug, Clone, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedReleaseValues {
	/// Per-package resolutions, keyed by package id.
	pub(crate) packages: BTreeMap<String, PackageValues>,
	/// Context values every value and label was computed from.
	pub(crate) label_inputs: LabelInputs,
}

impl ResolvedReleaseValues {
	/// Flatten every package's values into one map for template rendering.
	///
	/// Values are keyed by package, so the flattened form keeps the last
	/// package's value for a shared id and is only used for single-package
	/// surfaces.
	#[must_use]
	pub fn values_for(&self, package_id: &str) -> BTreeMap<String, String> {
		self.packages
			.get(package_id)
			.map_or_else(BTreeMap::new, |package| package.values.clone())
	}

	/// Every counter file write-back collected across packages.
	#[must_use]
	pub fn write_backs(&self) -> Vec<CounterWriteBack> {
		self.packages
			.values()
			.flat_map(|package| package.write_backs.iter().cloned())
			.collect()
	}

	/// Whether the release rendered any value or label at all.
	///
	/// A package appears in `packages` as soon as it is released, so emptiness
	/// is about rendered content rather than the map's size.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		self.packages
			.values()
			.all(|package| package.values.is_empty() && package.label.is_none())
	}

	/// Flatten every package's values into one map keyed by value id.
	///
	/// Used to freeze the release's values into the release record.
	#[must_use]
	pub fn frozen_values(&self) -> BTreeMap<String, String> {
		self.packages
			.iter()
			.flat_map(|(package_id, package)| {
				package.values.iter().map(move |(value_id, value)| {
					(format!("{package_id}.{value_id}"), value.clone())
				})
			})
			.collect()
	}

	/// Rendered display labels keyed by package id.
	#[must_use]
	pub fn frozen_labels(&self) -> BTreeMap<String, String> {
		self.packages
			.iter()
			.filter_map(|(package_id, package)| {
				package
					.label
					.clone()
					.map(|label| (package_id.clone(), label))
			})
			.collect()
	}
}

/// Inputs the resolver needs beyond configuration.
pub(crate) struct ResolveContext<'a> {
	/// Workspace root.
	pub(crate) root: &'a Path,
	/// Release timestamp the calendar context derives from.
	pub(crate) timestamp: ReleaseTimestamp,
	/// Previous release record's label inputs, used for ordinal chaining.
	pub(crate) previous_inputs: Option<&'a LabelInputs>,
	/// Identity version of the previous release of this owner, for train resets.
	pub(crate) previous_version: Option<&'a str>,
	/// Commit the release is prepared from, for git-derived values.
	pub(crate) commit: Option<&'a str>,
	/// Commit time in UTC, for `timestamp = "commit"`.
	pub(crate) commit_timestamp: Option<ReleaseTimestamp>,
}

/// Resolve every declared value and display label for the releasing packages.
///
/// `released` maps package id to identity version. Counters only advance for
/// packages that appear in it; every other package is left untouched.
pub(crate) async fn resolve_release_values(
	context: &ResolveContext<'_>,
	packages: &[PackageDefinition],
	schemes: &BTreeMap<String, VersionSchemeDefinition>,
	released: &BTreeMap<String, String>,
) -> MonochangeResult<ResolvedReleaseValues> {
	let label_inputs = chain_label_inputs(context.previous_inputs, context.timestamp);
	let mut resolved = ResolvedReleaseValues {
		packages: BTreeMap::new(),
		label_inputs: label_inputs.clone(),
	};

	for package in packages {
		let Some(version) = released.get(&package.id) else {
			continue;
		};
		let package_values =
			resolve_package_values(context, package, scheme_for(package, schemes)?, version)
				.await?;
		resolved.packages.insert(package.id.clone(), package_values);
	}
	Ok(resolved)
}

fn scheme_for<'a>(
	package: &PackageDefinition,
	schemes: &'a BTreeMap<String, VersionSchemeDefinition>,
) -> MonochangeResult<Option<&'a VersionSchemeDefinition>> {
	let Some(scheme_id) = package.display_version.as_deref() else {
		return Ok(None);
	};
	schemes.get(scheme_id).map(Some).ok_or_else(|| {
		MonochangeError::Config(format!(
			"package `{}` references unknown version scheme `{scheme_id}`",
			package.id
		))
	})
}

async fn resolve_package_values(
	context: &ResolveContext<'_>,
	package: &PackageDefinition,
	scheme: Option<&VersionSchemeDefinition>,
	version: &str,
) -> MonochangeResult<PackageValues> {
	let mut resolved = PackageValues::default();
	let mut write_backs = Vec::new();
	let mut monotonic = true;

	for (value_id, definition) in &package.values {
		let value = resolve_value(context, package, value_id, definition, version).await?;
		if let Some(write_back) = value.write_back {
			write_backs.push(write_back);
		}
		if !value.monotonic {
			monotonic = false;
		}
		resolved.values.insert(value_id.clone(), value.value);
	}
	resolved.write_backs = write_backs;
	resolved.monotonic = monotonic;

	if let Some(scheme) = scheme {
		let mut variables = monochange_core::versioning::template_variables(
			version,
			prerelease_of(version),
			&package.id,
			package.package_type.as_str(),
			&chain_label_inputs(context.previous_inputs, context.timestamp),
			&resolved.values,
		);
		variables.insert(
			"label".to_string(),
			render_version_template(&scheme.template, &variables),
		);
		resolved.label = Some(render_version_template(&scheme.template, &variables));
	}
	Ok(resolved)
}

async fn resolve_value(
	context: &ResolveContext<'_>,
	package: &PackageDefinition,
	value_id: &str,
	definition: &ValueDefinition,
	version: &str,
) -> MonochangeResult<ResolvedValue> {
	if let Some(file) = definition.file.as_deref() {
		return resolve_file_counter(context, package, value_id, definition, file, version).await;
	}
	if let Some(hash_path) = definition.hash.as_deref() {
		let full_path = context.root.join(hash_path);
		let bytes = tokio::fs::read(&full_path).await.map_err(|error| {
			MonochangeError::IoSource {
				path: full_path.clone(),
				source: error,
			}
		})?;
		let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
		let _ = HashAlgorithm::Sha256;
		return Ok(ResolvedValue {
			value: encode_hash(digest.as_ref(), definition.encoding, definition.length),
			monotonic: false,
			write_back: None,
		});
	}
	if let Some(variable) = definition.env.as_deref() {
		let value = std::env::var(variable).map_err(|_| {
			MonochangeError::Config(format!(
				"package `{}` value `{value_id}` reads environment variable `{variable}`, which is not set",
				package.id
			))
		})?;
		return Ok(ResolvedValue {
			value,
			monotonic: false,
			write_back: None,
		});
	}
	if let Some(git) = definition.git {
		let commit = context.commit.ok_or_else(|| {
			MonochangeError::Config(format!(
				"package `{}` value `{value_id}` needs the release commit but none was resolved",
				package.id
			))
		})?;
		let value = match git {
			monochange_core::versioning::GitSource::ShortHash => {
				commit.get(0..7).unwrap_or(commit).to_string()
			}
			monochange_core::versioning::GitSource::CommitCount => {
				let count = crate::git_support::run_git_capture(
					context.root,
					&["rev-list", "--count", commit],
					"count commits for the release value",
				)
				.await?;
				count.trim().to_string()
			}
			// patch-coverage:ignore-start -- unreachable fallback for a non_exhaustive enum.
			_ => unreachable!("unsupported git source"),
			// patch-coverage:ignore-end
		};
		return Ok(ResolvedValue {
			value,
			monotonic: false,
			write_back: None,
		});
	}
	if let Some(timestamp) = definition.timestamp {
		let stamp = match timestamp {
			monochange_core::versioning::TimestampSource::Now => context.timestamp,
			monochange_core::versioning::TimestampSource::Commit => {
				context.commit_timestamp.ok_or_else(|| {
					MonochangeError::Config(format!(
						"package `{}` value `{value_id}` reads the commit timestamp but none was resolved",
						package.id
					))
				})?
			}
			// patch-coverage:ignore-start -- unreachable fallback for a non_exhaustive enum.
			_ => unreachable!("unsupported timestamp source"),
			// patch-coverage:ignore-end
		};
		return Ok(ResolvedValue {
			value: format!("{}{}", stamp.date_compact(), stamp.time()),
			monotonic: false,
			write_back: None,
		});
	}
	Err(MonochangeError::Config(format!(
		"package `{}` value `{value_id}` declares no source",
		package.id
	)))
}

async fn resolve_file_counter(
	context: &ResolveContext<'_>,
	package: &PackageDefinition,
	value_id: &str,
	definition: &ValueDefinition,
	file: &Path,
	version: &str,
) -> MonochangeResult<ResolvedValue> {
	let field = definition.field.as_deref().unwrap_or_default();
	let full_path = context.root.join(file);
	let text = tokio::fs::read_to_string(&full_path)
		.await
		.map_err(|error| {
			if error.kind() == std::io::ErrorKind::NotFound {
				MonochangeError::Config(format!(
					"package `{}` value `{value_id}` reads counter file `{}`, which does not exist; create it with its starting value, for example {{\"build\": 0}}",
					package.id,
					file.display()
				))
			} else {
				MonochangeError::IoSource {
					path: full_path.clone(),
					source: error,
				}
			}
		})?;
	let document: serde_json::Value = serde_json::from_str(&text).map_err(|error| {
		MonochangeError::Config(format!(
			"package `{}` value `{value_id}` could not parse counter file `{}` as JSON: {error}",
			package.id,
			file.display()
		))
	})?;
	let current = counter_from_json(&document, file, field)?;

	let behaviour = definition.stamp_behaviour();
	if !behaviour.is_stamped() {
		return Ok(ResolvedValue {
			value: current.to_string(),
			monotonic: false,
			write_back: None,
		});
	}

	// Train-scoped counters restart when the identity version changes.
	let resets =
		definition.reset == ResetPolicy::Version && train_reset(context.previous_version, version);
	let next = if resets { 1 } else { behaviour.apply(current) };

	Ok(ResolvedValue {
		value: next.to_string(),
		monotonic: true,
		write_back: Some(CounterWriteBack {
			file: file.to_path_buf(),
			field: field.to_string(),
			value: next,
		}),
	})
}

/// Decide whether a train-scoped counter must reset.
///
/// The previous release record supplies the identity that produced the stored
/// counter. A different identity starts the counter over at `1`, which is
/// Apple's release-train rule for iOS build numbers.
fn train_reset(previous_version: Option<&str>, version: &str) -> bool {
	previous_version.is_some_and(|previous| previous != version)
}

fn prerelease_of(version: &str) -> &str {
	version.split_once('-').map_or("", |(_, prerelease)| {
		prerelease.split('+').next().unwrap_or(prerelease)
	})
}

/// Apply counter write-backs to their files, preserving surrounding content.
pub(crate) async fn apply_counter_write_backs(
	root: &Path,
	write_backs: &[CounterWriteBack],
) -> MonochangeResult<Vec<PathBuf>> {
	let mut written = Vec::new();
	for write_back in write_backs {
		let full_path = root.join(&write_back.file);
		let text = tokio::fs::read_to_string(&full_path)
			.await
			.map_err(|error| {
				MonochangeError::IoSource {
					path: full_path.clone(),
					source: error,
				}
			})?;
		let updated =
			rewrite_counter_field(&text, &write_back.field, write_back.value).map_err(|error| {
				MonochangeError::Config(format!(
					"could not update counter field `{}` in `{}`: {error}",
					write_back.field,
					write_back.file.display()
				))
			})?;
		tokio::fs::write(&full_path, updated)
			.await
			.map_err(|error| {
				MonochangeError::IoSource {
					path: full_path.clone(),
					source: error,
				}
			})?;
		if !written.contains(&write_back.file) {
			written.push(write_back.file.clone());
		}
	}
	Ok(written)
}

/// Replace one integer field in a JSON document while preserving formatting.
///
/// The rewrite is a targeted text substitution rather than a re-serialisation,
/// so key order, indentation, and any comments-as-keys survive.
fn rewrite_counter_field(text: &str, field: &str, value: u64) -> Result<String, String> {
	let segments = monochange_core::versioning::counter_field_segments(field);
	let Some((leaf, parents)) = segments.split_last() else {
		return Err("field path has no segments".to_string());
	};
	if !parents.is_empty() {
		// Nested fields are rewritten by parsing and re-serialising only the
		// matched value, which keeps the common flat case formatting-stable.
		let mut document: serde_json::Value =
			serde_json::from_str(text).map_err(|error| error.to_string())?;
		let mut cursor = &mut document;
		for segment in parents {
			cursor = cursor
				.get_mut(*segment)
				.ok_or_else(|| format!("missing object segment `{segment}`"))?;
		}
		let object = cursor
			.as_object_mut()
			.ok_or_else(|| format!("field parent for `{field}` is not an object"))?;
		object.insert((*leaf).to_string(), serde_json::Value::from(value));
		return Ok(format!(
			"{}\n",
			serde_json::to_string_pretty(&document).map_err(|error| error.to_string())?
		));
	}

	// Flat field: replace the first `"<leaf>": <digits>` occurrence.
	let pattern = format!("\"{leaf}\"");
	let Some(key_start) = text.find(&pattern) else {
		return Err(format!("field `{leaf}` was not found"));
	};
	let after_key = key_start + pattern.len();
	let Some(colon_offset) = text[after_key..].find(':') else {
		return Err(format!("field `{leaf}` has no value"));
	};
	let value_start = after_key + colon_offset + 1;
	let remainder = &text[value_start..];
	let leading = remainder.len() - remainder.trim_start().len();
	let digits_start = value_start + leading;
	let digits_len = text[digits_start..]
		.chars()
		.take_while(char::is_ascii_digit)
		.count();
	if digits_len == 0 {
		return Err(format!("field `{leaf}` does not hold a number"));
	}
	let mut updated = String::with_capacity(text.len());
	updated.push_str(&text[..digits_start]);
	updated.push_str(&value.to_string());
	updated.push_str(&text[digits_start + digits_len..]);
	Ok(updated)
}

#[cfg(test)]
#[path = "__tests__/versioning_state_tests.rs"]
mod tests;
