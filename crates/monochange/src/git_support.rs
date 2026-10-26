use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use monochange_core::CommitMessage;
use monochange_core::DiscoveryPathFilter;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::git::git_command_output;
use monochange_core::git::git_error_detail;
use monochange_core::git::git_stage_all_command;
use monochange_core::git::git_stage_paths_command;
use monochange_core::git::git_stderr_trimmed;
use monochange_core::git::git_stdout_trimmed;
use monochange_core::git::run_git_commit_message;
#[must_use = "the tag commit result must be checked"]
pub(crate) async fn resolve_git_tag_commit(
	root: &Path,
	tag_name: &str,
) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&[
			"rev-parse",
			"--verify",
			&format!("refs/tags/{tag_name}^{{commit}}"),
		],
		&format!("release tag {tag_name} could not be found"),
	)
	.await
}

pub(crate) async fn git_is_ancestor(
	root: &Path,
	ancestor: &str,
	descendant: &str,
) -> MonochangeResult<bool> {
	let output = git_command_output(root, &["merge-base", "--is-ancestor", ancestor, descendant])
		.await
		.map_err(|error| {
			MonochangeError::Discovery(format!("failed to compare commit ancestry: {error}"))
		})?;

	match output.status.code() {
		Some(0) => Ok(true),
		Some(1) => Ok(false),
		_ => Err(MonochangeError::Discovery(git_stderr_trimmed(&output))),
	}
}

pub(crate) async fn create_git_tag(
	root: &Path,
	tag_name: &str,
	target_commit: &str,
) -> MonochangeResult<()> {
	run_git_status(
		root,
		&["tag", tag_name, target_commit],
		&format!("failed to create tag `{tag_name}`"),
	)
	.await
}

pub(crate) async fn move_git_tag(
	root: &Path,
	tag_name: &str,
	target_commit: &str,
) -> MonochangeResult<()> {
	run_git_status(
		root,
		&["tag", "--force", tag_name, target_commit],
		&format!("failed to retarget tag `{tag_name}`"),
	)
	.await
}

#[must_use = "the push result must be checked"]
pub(crate) async fn push_git_tags(root: &Path, tags: &[&str]) -> MonochangeResult<()> {
	push_git_tags_with_options(root, tags, true, "failed to push retargeted release tags").await
}

#[must_use = "the push result must be checked"]
pub(crate) async fn push_git_tags_without_force(
	root: &Path,
	tags: &[&str],
) -> MonochangeResult<()> {
	push_git_tags_with_options(root, tags, false, "failed to push release tags").await
}

async fn push_git_tags_with_options(
	root: &Path,
	tags: &[&str],
	force: bool,
	error_message: &str,
) -> MonochangeResult<()> {
	let mut args = vec!["push"];

	if force {
		args.push("--force");
	}

	args.push("origin");

	let tag_refs: Vec<String> = tags
		.iter()
		.map(|tag| format!("refs/tags/{tag}:refs/tags/{tag}"))
		.collect();

	for tag_ref in &tag_refs {
		args.push(tag_ref.as_str());
	}

	run_git_status(root, &args, error_message).await
}

#[must_use = "the ref resolution result must be checked"]
pub(crate) async fn resolve_git_commit_ref(root: &Path, from: &str) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["rev-parse", "--verify", &format!("{from}^{{commit}}")],
		&format!("could not resolve ref `{from}` to a commit"),
	)
	.await
}

#[must_use = "the commit history result must be checked"]
#[tracing::instrument(skip_all, fields(commit))]
pub(crate) async fn first_parent_history(
	root: &Path,
	commit: &str,
) -> MonochangeResult<FirstParentHistory> {
	let output = run_git_capture(
		root,
		&["log", "--first-parent", "--format=%H %P", commit],
		"failed to read first-parent commit ancestry",
	)
	.await?;

	let mut entries = Vec::new();
	for line in output.lines().filter(|line| !line.is_empty()) {
		let (commit, parents) = line.split_once(' ').unwrap_or((line, ""));
		let first_parent = parents
			.split(' ')
			.next()
			.filter(|parent| !parent.is_empty())
			.map(str::to_string);
		entries.push(FirstParentEntry {
			commit: commit.to_string(),
			first_parent,
		});
	}

	Ok(FirstParentHistory::from_entries(entries))
}

/// One commit in a first-parent walk plus its first parent.
///
/// `first_parent` is `None` at a graft boundary: the root commit, or the cut
/// of a shallow clone, where git reports no parents for the boundary commit.
pub(crate) struct FirstParentEntry {
	pub(crate) commit: String,
	pub(crate) first_parent: Option<String>,
}

impl std::fmt::Debug for FirstParentEntry {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter
			.debug_struct("FirstParentEntry")
			.field("commit", &self.commit)
			.field("first_parent", &self.first_parent)
			.finish()
	}
}

/// First-parent ancestry resolved by one `git log` run.
///
/// The walk also remembers which commits it contains, so a commit whose
/// recorded first parent never appears in the walk is recognized as sitting
/// at a cut boundary — the parent object is not available to diff against.
pub(crate) struct FirstParentHistory {
	entries: Vec<FirstParentEntry>,
	walk_commits: std::collections::HashSet<String>,
}

impl std::fmt::Debug for FirstParentHistory {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter
			.debug_struct("FirstParentHistory")
			.field("entries", &self.entries)
			.field("walk_commits", &self.walk_commits)
			.finish()
	}
}

impl FirstParentHistory {
	fn from_entries(entries: Vec<FirstParentEntry>) -> Self {
		let walk_commits = entries
			.iter()
			.map(|entry| entry.commit.clone())
			.collect::<std::collections::HashSet<_>>();
		Self {
			entries,
			walk_commits,
		}
	}

	pub(crate) fn iter(&self) -> impl Iterator<Item = &FirstParentEntry> {
		self.entries.iter()
	}

	fn contains_commit(&self, commit: &str) -> bool {
		self.walk_commits.contains(commit)
	}
}

pub(crate) async fn read_git_file_at_commit(
	root: &Path,
	commit: &str,
	path: &str,
) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["show", &format!("{commit}:{path}")],
		&format!("failed to read `{path}` at commit `{commit}`"),
	)
	.await
}

pub(crate) async fn find_release_record_files_at_commit(
	root: &Path,
	entry: &FirstParentEntry,
	history: &FirstParentHistory,
) -> MonochangeResult<Vec<String>> {
	let prefix = ".monochange/releases/";
	let suffix = "/release.json";
	let filter = |line: &&str| line.starts_with(prefix) && line.ends_with(suffix);

	if let Some(parent) = entry.first_parent.as_deref()
		&& history.contains_commit(parent)
	{
		// Exclude deleted paths: a commit that removes a release record (for
		// example when reverting a release preparation) must not make the
		// discovery walk treat the deleted record as present at that commit.
		//
		// Diff against the first parent only. `-m` diffs a merge commit
		// against every parent, so a branch that was cut before a release
		// re-reports that release's record relative to its older second
		// parent, and every post-release merge of such a branch looks like a
		// new release commit.
		let args = [
			"diff-tree",
			"--no-commit-id",
			"--name-only",
			"-r",
			"--diff-filter=d",
			parent,
			&entry.commit,
		];
		let output = run_git_capture(root, &args, "failed to list files at commit").await?;
		Ok(output
			.lines()
			.filter(filter)
			.map(str::to_string)
			.collect::<std::collections::HashSet<_>>()
			.into_iter()
			.collect())
	} else {
		// The parent object is unavailable (root or shallow cut), so the only
		// way to see which records this commit carries is its full tree.
		let args = ["ls-tree", "-r", "--name-only", &entry.commit];
		let output = run_git_capture(root, &args, "failed to list files at commit").await?;
		Ok(output.lines().filter(filter).map(str::to_string).collect())
	}
}

#[allow(dead_code)]
#[must_use = "the commit message result must be checked"]
pub(crate) async fn read_git_commit_message(root: &Path, commit: &str) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["show", "-s", "--format=%B", commit],
		&format!("failed to read commit message for `{commit}`"),
	)
	.await
}

#[tracing::instrument(skip_all, fields(args = ?args))]
pub(crate) async fn run_git_capture(
	root: &Path,
	args: &[&str],
	error_message: &str,
) -> MonochangeResult<String> {
	let output = git_command_output(root, args)
		.await
		.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;

	if !output.status.success() {
		let stderr = git_stderr_trimmed(&output);
		tracing::debug!(args = ?args, %stderr, "git command failed");

		return Err(MonochangeError::Discovery(git_error_message_with_detail(
			error_message,
			&stderr,
		)));
	}

	Ok(git_stdout_trimmed(&output))
}

pub(crate) async fn run_git_status(
	root: &Path,
	args: &[&str],
	error_message: &str,
) -> MonochangeResult<()> {
	run_git_capture(root, args, error_message).await.map(|_| ())
}

#[must_use = "the staging result must be checked"]
pub(crate) async fn git_stage_paths(
	root: &Path,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<()> {
	let stageable_paths = resolve_stageable_release_paths(root, tracked_paths).await?;

	if stageable_paths.is_empty() {
		let skipped_count = tracked_paths.len();
		tracing::debug!(
			count = skipped_count,
			"no release commit paths required staging"
		);

		return Ok(());
	}

	let stageable_count = stageable_paths.len();
	tracing::debug!(
		count = stageable_count,
		?stageable_paths,
		"staging release commit paths"
	);

	run_git_process(
		git_stage_paths_command(root, &stageable_paths),
		"failed to stage release commit files",
	)
	.await
}

// patch-coverage:ignore-start -- thin git command wrapper covered by git_support integration tests.
#[must_use = "the staging result must be checked"]
pub(crate) async fn git_stage_all(root: &Path) -> MonochangeResult<()> {
	run_git_process(
		git_stage_all_command(root),
		"failed to stage release commit files",
	)
	.await
}
// patch-coverage:ignore-end
/// Decide which release paths `git add` must stage, asking git about every
/// path in two batched calls instead of one call per path.
///
/// The per-path rules match the original sequential form: paths the discovery
/// filter ignores never stage; missing paths stage only when git still tracks
/// them (staging a deletion); `.monochange/releases` paths always stage; and
/// any other path stages when git tracks it or has not ignored it.
pub(crate) async fn resolve_stageable_release_paths(
	root: &Path,
	tracked_paths: &[PathBuf],
) -> MonochangeResult<Vec<PathBuf>> {
	/// What a path needs from git before it can stage.
	enum Pending {
		/// A path that no longer exists stages only when git tracks it.
		TrackedOnly,
		/// An existing path outside `.monochange/releases` stages when git
		/// tracks it, and otherwise when git has not ignored it.
		TrackedOrUnignored,
	}

	let path_filter = DiscoveryPathFilter::new(root);
	let mut pending = Vec::<(&PathBuf, Pending)>::new();
	let mut stageable_paths = Vec::with_capacity(tracked_paths.len());

	for path in tracked_paths {
		let relative = root_relative_path(root, path);

		if !relative.starts_with(".monochange/releases")
			&& release_path_is_ignored_by_filter(root, relative, &path_filter)
		{
			tracing::debug!(path = %path.display(), "skipping non-stageable release path");
			continue;
		}

		if !root.join(path).exists() {
			pending.push((path, Pending::TrackedOnly));
		} else if relative.starts_with(".monochange/releases") {
			stageable_paths.push(path.clone());
		} else {
			pending.push((path, Pending::TrackedOrUnignored));
		}
	}

	if pending.is_empty() {
		return Ok(stageable_paths);
	}

	let mut queried_paths = Vec::with_capacity(pending.len());
	for (path, _) in &pending {
		queried_paths.push((*path).clone());
	}
	let tracked = git_tracked_paths(root, &queried_paths).await?;

	// Untracked existing paths still need the ignore check; ask about all of
	// them in one call.
	let mut untracked_unignored_queries = Vec::new();
	for (path, requirement) in &pending {
		if matches!(requirement, Pending::TrackedOrUnignored)
			&& !tracked.contains(&git_path_key(root, path))
		{
			untracked_unignored_queries.push((*path).clone());
		}
	}
	let ignored = if untracked_unignored_queries.is_empty() {
		std::collections::HashSet::new()
	} else {
		git_ignored_paths(root, &untracked_unignored_queries).await?
	};

	// Preserve the input order the sequential form produced.
	for (path, requirement) in pending {
		let path_key = git_path_key(root, path);
		let stageable = match requirement {
			Pending::TrackedOnly => tracked.contains(&path_key),
			Pending::TrackedOrUnignored => {
				tracked.contains(&path_key) || !ignored.contains(&path_key)
			}
		};

		if stageable {
			stageable_paths.push(path.clone());
		} else {
			tracing::debug!(path = %path.display(), "skipping non-stageable release path");
		}
	}

	Ok(stageable_paths)
}

fn release_path_is_ignored_by_filter(
	root: &Path,
	relative: &Path,
	path_filter: &DiscoveryPathFilter,
) -> bool {
	!path_filter.allows(&root.join(relative))
}

fn root_relative_path<'a>(root: &Path, path: &'a Path) -> &'a Path {
	if path.is_absolute() {
		path.strip_prefix(root).unwrap_or(path)
	} else {
		path
	}
}

/// The key a path compares under in git path listings: root-relative with
/// git's forward-slash separators, regardless of how the caller built the
/// path. Absolute paths inside the repository and `\\`-separated Windows
/// paths then match the same entry git prints.
fn git_path_key(root: &Path, path: &Path) -> String {
	root_relative_path(root, path)
		.to_string_lossy()
		.replace('\\', "/")
}

/// Report which of `paths` git tracks, in one `git ls-files` run.
///
/// `ls-files` reads the index, so deleted-but-tracked paths are still
/// reported, matching the per-path `--error-unmatch` check it replaces.
#[must_use = "the tracked paths result must be checked"]
async fn git_tracked_paths(
	root: &Path,
	paths: &[PathBuf],
) -> MonochangeResult<std::collections::HashSet<String>> {
	let mut args = vec!["ls-files".to_string(), "--".to_string()];
	args.extend(paths.iter().map(|path| path.to_string_lossy().into_owned()));
	let args = args.iter().map(String::as_str).collect::<Vec<_>>();
	let output = git_command_output(root, &args).await.map_err(|error| {
		MonochangeError::Discovery(format!("failed to inspect tracked git paths: {error}"))
	})?;

	if !output.status.success() {
		return Err(MonochangeError::Discovery(format!(
			"failed to inspect tracked git paths: {}",
			git_error_detail(&output)
		)));
	}

	let stdout = String::from_utf8_lossy(&output.stdout);
	Ok(stdout
		.lines()
		.filter(|line| !line.is_empty())
		.map(str::to_string)
		.collect())
}

/// Report which of `paths` git ignores, in one `git check-ignore` run.
///
/// `check-ignore` exits 1 when no path is ignored and prints each ignored
/// path otherwise; both carry the same information as the per-path quiet
/// check it replaces.
#[must_use = "the ignored paths result must be checked"]
async fn git_ignored_paths(
	root: &Path,
	paths: &[PathBuf],
) -> MonochangeResult<std::collections::HashSet<String>> {
	let mut args = vec!["check-ignore".to_string(), "--".to_string()];
	args.extend(paths.iter().map(|path| path.to_string_lossy().into_owned()));
	let args = args.iter().map(String::as_str).collect::<Vec<_>>();
	let output = git_command_output(root, &args).await.map_err(|error| {
		MonochangeError::Discovery(format!("failed to inspect ignored git paths: {error}"))
	})?;

	match output.status.code() {
		Some(0 | 1) => {}
		_ => {
			return Err(MonochangeError::Discovery(format!(
				"failed to inspect ignored git paths: {}",
				git_error_detail(&output)
			)));
		}
	}

	let stdout = String::from_utf8_lossy(&output.stdout);
	Ok(stdout
		.lines()
		.filter(|line| !line.is_empty())
		.map(str::to_string)
		.collect())
}

#[must_use = "the commit result must be checked"]
pub(crate) async fn git_commit_paths(
	root: &Path,
	message: &CommitMessage,
	no_verify: bool,
) -> MonochangeResult<()> {
	run_git_commit_message(root, message, "create release commit", no_verify).await
}

/// List every path `git add --all` would put into the next commit, relative
/// to `root`, without touching the index.
///
/// Tracked changes, staged or not, come from `git diff HEAD`; renames are
/// split into a deletion and an addition so every path is reported on its
/// own. Untracked files are included unless `.gitignore` excludes them.
#[must_use = "the changed paths must be used"]
pub(crate) async fn git_changed_paths(root: &Path) -> MonochangeResult<Vec<PathBuf>> {
	let tracked = run_git_capture(
		root,
		&[
			"diff",
			"--name-only",
			"--no-renames",
			"--relative",
			"-z",
			"HEAD",
		],
		"failed to list changed release files",
	)
	.await?;
	let untracked = run_git_capture(
		root,
		&["ls-files", "--others", "--exclude-standard", "-z"],
		"failed to list untracked release files",
	)
	.await?;
	let mut paths = tracked
		.split('\0')
		.chain(untracked.split('\0'))
		.filter(|path| !path.is_empty())
		.map(PathBuf::from)
		.collect::<Vec<_>>();
	paths.sort();
	paths.dedup();

	Ok(paths)
}

#[must_use = "the HEAD commit result must be checked"]
pub(crate) async fn git_head_commit(root: &Path) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["rev-parse", "HEAD"],
		"failed to read release commit sha",
	)
	.await
}

pub(crate) async fn run_git_process(
	command: ProcessCommand,
	error_message: &str,
) -> MonochangeResult<()> {
	let output = tokio::process::Command::from(command)
		.output()
		.await
		.map_err(|error| MonochangeError::Discovery(format!("{error_message}: {error}")))?;
	handle_git_process_output(&output, error_message)
}

fn git_error_message_with_detail(error_message: &str, stderr: &str) -> String {
	if error_message.is_empty() {
		return stderr.to_string();
	}

	if stderr.is_empty() {
		return error_message.to_string();
	}

	let mut detail = String::with_capacity(error_message.len() + 2 + stderr.len());
	detail.push_str(error_message);
	detail.push_str(": ");
	detail.push_str(stderr);
	detail
}

fn handle_git_process_output(
	output: &std::process::Output,
	error_message: &str,
) -> MonochangeResult<()> {
	if !output.status.success() {
		let stderr = git_error_detail(output);
		return Err(MonochangeError::Discovery(git_error_message_with_detail(
			error_message,
			&stderr,
		)));
	}

	Ok(())
}

#[cfg(test)]
#[path = "__tests__/git_support_tests.rs"]
mod tests;
