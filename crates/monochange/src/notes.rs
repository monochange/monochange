use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::DEFAULT_CHANGELOG_OUTPUT;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::WorkspaceConfiguration;

use crate::PreparedChangelog;
use crate::prepare_release_execution_with_configuration;

pub(crate) async fn render_notes(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	output_id: &str,
	target_id: Option<&str>,
	file: Option<&Path>,
) -> MonochangeResult<String> {
	validate_selection(configuration, output_id, target_id)?;

	let execution =
		prepare_release_execution_with_configuration(root, configuration, true, false, false)
			.await?;
	let mut matches = execution
		.prepared_release
		.changelogs
		.iter()
		.filter(|changelog| changelog.output == output_id)
		.filter(|changelog| target_id.is_none_or(|target| changelog.owner_id == target));
	let Some(changelog) = matches.next() else {
		return Err(MonochangeError::Config(no_notes_message(
			output_id, target_id,
		)));
	};
	if matches.next().is_some() {
		return Err(MonochangeError::Config(format!(
			"changelog output `{output_id}` produced release notes for multiple targets; pass `--target <id>` to select one"
		)));
	}

	write_or_render(root, changelog, file)
}

fn validate_selection(
	configuration: &WorkspaceConfiguration,
	output_id: &str,
	target_id: Option<&str>,
) -> MonochangeResult<()> {
	if output_id == DEFAULT_CHANGELOG_OUTPUT {
		return Ok(());
	}

	let Some(output) = configuration.changelog.outputs.get(output_id) else {
		let available = std::iter::once(DEFAULT_CHANGELOG_OUTPUT)
			.chain(configuration.changelog.outputs.keys().map(String::as_str))
			.collect::<BTreeSet<_>>()
			.into_iter()
			.collect::<Vec<_>>()
			.join(", ");
		return Err(MonochangeError::Config(format!(
			"unknown changelog output `{output_id}`; available outputs: {available}"
		)));
	};

	if let Some(target_id) = target_id
		&& !output.targets.iter().any(|target| target == target_id)
	{
		return Err(MonochangeError::Config(format!(
			"changelog output `{output_id}` does not target `{target_id}`; configured targets: {}",
			output.targets.join(", ")
		)));
	}

	Ok(())
}

fn no_notes_message(output_id: &str, target_id: Option<&str>) -> String {
	target_id.map_or_else(
		|| format!("changelog output `{output_id}` produced no release notes"),
		|target| {
			format!(
				"changelog output `{output_id}` produced no release notes for target `{target}`"
			)
		},
	)
}

fn write_or_render(
	root: &Path,
	changelog: &PreparedChangelog,
	file: Option<&Path>,
) -> MonochangeResult<String> {
	let Some(file) = file else {
		return Ok(changelog.rendered.clone());
	};
	if file == Path::new("-") {
		return Ok(changelog.rendered.clone());
	}

	let path = absolute_output_path(root, file);
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to create release-notes directory {}: {error}",
				parent.display()
			))
		})?;
	}
	let contents = format!("{}\n", changelog.rendered);
	fs::write(&path, contents).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write release notes to {}: {error}",
			path.display()
		))
	})?;

	Ok(String::new())
}

fn absolute_output_path(root: &Path, file: &Path) -> PathBuf {
	if file.is_absolute() {
		return file.to_path_buf();
	}
	root.join(file)
}
