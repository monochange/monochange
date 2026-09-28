#![allow(clippy::disallowed_methods)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use tempfile::tempdir;

use super::*;

/// The published skill package that `scripts/docs/sync-skill.mjs` copies from.
fn skill_package_dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("..")
		.join("..")
		.join("packages")
		.join("monochange__skill")
}

fn read_source(file: &str) -> Vec<u8> {
	let path = skill_package_dir().join(file);
	fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn topic_file(topics: &[Topic], name: &str) -> &'static str {
	topics.iter().find(|topic| topic.name == name).map_or_else(
		|| panic!("topic `{name}` must be bundled"),
		|topic| topic.file,
	)
}

#[test]
fn every_reference_is_bundled_with_substantive_content() {
	let topics = topics();
	assert_eq!(
		topics.len(),
		18,
		"the entrypoint plus twelve skill modules and five examples"
	);

	let mut names = std::collections::BTreeSet::new();
	let mut files = std::collections::BTreeSet::new();
	for topic in topics {
		assert!(
			names.insert(topic.name),
			"topic name `{}` is duplicated",
			topic.name
		);
		assert!(
			files.insert(topic.file),
			"install path `{}` is duplicated",
			topic.file
		);
		assert!(
			topic.content.len() > 200,
			"`{}` looks truncated at {} bytes",
			topic.file,
			topic.content.len()
		);
		assert!(
			topic.content.ends_with('\n'),
			"`{}` must end with a newline so read output stays verbatim Markdown",
			topic.file
		);
		if topic.name != "monochange" {
			assert!(
				topic.content.starts_with("# "),
				"`{}` must start with a Markdown heading",
				topic.file
			);
		}
	}
}

#[test]
fn entrypoint_is_the_bundled_skill_md() {
	assert!(SKILL_ENTRY.starts_with("---\nname: monochange\n"));
	assert!(SKILL_ENTRY.contains("# monochange"));
	assert_eq!(topic_file(topics(), "monochange"), "SKILL.md");
	assert_eq!(
		read_source("SKILL.md"),
		SKILL_ENTRY.as_bytes(),
		"the embedded entrypoint must be the published SKILL.md"
	);
}

#[test]
fn read_serves_each_topic_verbatim() {
	for topic in topics() {
		let outcome = run_skill(SkillAction::Read {
			topic: Some(topic.name.to_owned()),
		})
		.unwrap_or_else(|error| panic!("read `{}`: {error}", topic.name));
		let SkillOutcome::Read { content } = outcome else {
			panic!("reading `{}` must return a document", topic.name);
		};
		assert_eq!(
			content, topic.content,
			"`{}` must be served from the bundled table",
			topic.name
		);
		assert_eq!(
			content.as_bytes(),
			read_source(topic.file).as_slice(),
			"`{}` must be served byte-for-byte from packages/monochange__skill",
			topic.name
		);
	}
}

#[test]
fn read_rejects_an_unknown_topic_with_the_available_names() {
	let error = run_skill(SkillAction::Read {
		topic: Some("anchor".to_owned()),
	})
	.err()
	.unwrap_or_else(|| panic!("unknown topic must fail"));
	let message = error.to_string();
	assert!(message.contains("unknown skill topic `anchor`"));
	for topic in topics() {
		assert!(
			message.contains(topic.name),
			"the error must name `{}`: {message}",
			topic.name
		);
	}
}

#[test]
fn listing_lists_every_topic_with_its_install_path() {
	let SkillOutcome::Listed(listing) =
		run_skill(SkillAction::List).unwrap_or_else(|error| panic!("list topics: {error}"))
	else {
		panic!("bare `monochange skill` must list topics");
	};
	assert!(listing.contains("monochange skill read <topic>"));
	assert!(listing.contains("monochange skill install --dir <dir>"));
	for topic in topics() {
		assert!(
			listing.contains(topic.name),
			"the listing must name `{}`",
			topic.name
		);
		assert!(
			listing.contains(topic.file),
			"the listing must show the install path `{}`",
			topic.file
		);
		assert!(
			listing.contains(topic.description),
			"the listing must describe `{}`",
			topic.name
		);
	}

	let outcome = run_skill(SkillAction::Read { topic: None })
		.unwrap_or_else(|error| panic!("read without a topic: {error}"));
	assert!(
		matches!(outcome, SkillOutcome::Listed(_)),
		"read without a topic must fall back to the listing"
	);
}

#[test]
fn render_outcome_writes_read_documents_verbatim() {
	let outcome = run_skill(SkillAction::Read {
		topic: Some("example-readme".to_owned()),
	})
	.unwrap_or_else(|error| panic!("read example-readme: {error}"));
	let rendered = render_outcome(outcome)
		.unwrap_or_else(|error| panic!("write example-readme to stdout: {error}"));
	assert!(
		rendered.is_empty(),
		"a read document is written raw, so the caller must not print it again"
	);
}

#[test]
fn install_writes_every_document_into_the_target_tree() {
	let root = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let destination = root.path().join("skills").join("monochange");

	let outcome = run_skill(SkillAction::Install {
		destination: Some(destination.clone()),
		force: false,
	})
	.unwrap_or_else(|error| panic!("install: {error}"));
	let SkillOutcome::Installed {
		destination: reported,
		files,
	} = outcome
	else {
		panic!("install must report the written tree");
	};
	assert_eq!(reported, destination);
	assert_eq!(files, topics().len());
	for topic in topics() {
		let path = destination.join(topic.file);
		let written =
			fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
		assert_eq!(
			written.as_slice(),
			topic.content.as_bytes(),
			"`{}` must round-trip byte-for-byte",
			topic.file
		);
	}
}

#[test]
fn install_refuses_to_replace_a_curated_skill_without_force() {
	let root = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let destination = root.path().join("monochange");
	fs::create_dir_all(&destination).unwrap_or_else(|error| panic!("create destination: {error}"));
	fs::write(destination.join("SKILL.md"), "curated by the operator")
		.unwrap_or_else(|error| panic!("seed curated skill: {error}"));

	let error = run_skill(SkillAction::Install {
		destination: Some(destination.clone()),
		force: false,
	})
	.err()
	.unwrap_or_else(|| panic!("existing skill must block"));
	assert!(error.to_string().contains("already contains a skill"));
	assert!(error.to_string().contains("--force"));
	let surviving = fs::read_to_string(destination.join("SKILL.md"))
		.unwrap_or_else(|error| panic!("read curated skill: {error}"));
	assert_eq!(
		surviving, "curated by the operator",
		"a refused install must not touch the curated skill"
	);

	let replaced = run_skill(SkillAction::Install {
		destination: Some(destination.clone()),
		force: true,
	})
	.unwrap_or_else(|error| panic!("forced install: {error}"));
	assert!(matches!(replaced, SkillOutcome::Installed { .. }));
	let overwritten = fs::read_to_string(destination.join("SKILL.md"))
		.unwrap_or_else(|error| panic!("read replaced skill: {error}"));
	assert!(overwritten.starts_with("---\nname: monochange"));
}

#[test]
fn install_without_a_destination_reports_usable_guidance() {
	let error = run_skill(SkillAction::Install {
		destination: None,
		force: false,
	})
	.err()
	.unwrap_or_else(|| panic!("missing destination must fail"));
	let message = error.to_string();
	assert!(message.contains("--dir"), "{message}");
	assert!(
		message.contains("monochange skill install --dir"),
		"{message}"
	);
	assert!(message.contains("skills/monochange"), "{message}");
}

#[test]
fn install_without_a_home_still_suggests_runtime_directories() {
	temp_env::with_var("HOME", None::<&str>, || {
		let error = run_skill(SkillAction::Install {
			destination: None,
			force: false,
		})
		.err()
		.unwrap_or_else(|| panic!("missing destination must fail"));
		let message = error.to_string();
		for runtime in [".claude", ".codex", ".agents"] {
			assert!(
				message.contains(&format!("~/{runtime}/skills/monochange")),
				"the fallback must name `{runtime}`: {message}"
			);
		}
	});
}

#[test]
fn install_reports_write_failures() {
	let root = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let blocker = root.path().join("blocker");
	fs::write(&blocker, "not a directory").unwrap_or_else(|error| panic!("write blocker: {error}"));

	let error = run_skill(SkillAction::Install {
		destination: Some(blocker.join("monochange")),
		force: false,
	})
	.err()
	.unwrap_or_else(|| panic!("install under a file must fail"));
	assert!(error.to_string().contains("failed to write the skill"));
}

#[test]
fn suggested_destinations_are_named_monochange_under_known_runtimes() {
	let suggested = suggest_destinations();
	if suggested.is_empty() {
		return; // no HOME in this environment
	}
	assert_eq!(suggested.len(), 3);
	for path in suggested {
		assert_eq!(
			path.file_name().and_then(|name| name.to_str()),
			Some("monochange"),
			"destinations must be named monochange, got {}",
			path.display()
		);
		let parent = path
			.parent()
			.and_then(Path::file_name)
			.and_then(|name| name.to_str());
		assert_eq!(
			parent,
			Some("skills"),
			"destinations must live under a skills directory, got {}",
			path.display()
		);
	}
}

/// A writer whose every operation fails, so the verbatim write paths are
/// exercised without needing a closed stdout.
struct FailingWriter;

impl std::io::Write for FailingWriter {
	fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
		Err(std::io::Error::other("write refused"))
	}

	fn flush(&mut self) -> std::io::Result<()> {
		Err(std::io::Error::other("flush refused"))
	}
}

#[test]
fn write_verbatim_reports_writer_and_flush_failures() {
	let write_error = write_verbatim_to(&mut FailingWriter, "document")
		.err()
		.unwrap_or_else(|| panic!("expected a write failure"));
	assert!(
		write_error
			.to_string()
			.contains("failed to write the skill document"),
		"unexpected write error: {write_error}"
	);

	// A writer that accepts the bytes but cannot flush covers the second path.
	struct Unflushable;
	impl std::io::Write for Unflushable {
		fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
			Ok(bytes.len())
		}

		fn flush(&mut self) -> std::io::Result<()> {
			Err(std::io::Error::other("flush refused"))
		}
	}
	let flush_error = write_verbatim_to(&mut Unflushable, "document")
		.err()
		.unwrap_or_else(|| panic!("expected a flush failure"));
	assert!(
		flush_error
			.to_string()
			.contains("failed to flush the skill document"),
		"unexpected flush error: {flush_error}"
	);
}

#[test]
fn install_reports_a_write_failure_instead_of_claiming_success() {
	let directory = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let destination = directory.path().join("monochange");

	// Install once so the tree exists, then make one document read-only. A
	// read-only *directory* is not enough on every platform, because rewriting
	// an existing file needs no directory write permission; a read-only file
	// fails `fs::write` deterministically.
	run_skill(SkillAction::Install {
		destination: Some(destination.clone()),
		force: false,
	})
	.unwrap_or_else(|error| panic!("initial install: {error}"));

	let document = destination.join("SKILL.md");
	let mode = std::fs::metadata(&document)
		.unwrap_or_else(|error| panic!("metadata: {error}"))
		.permissions()
		.mode();
	std::fs::set_permissions(&document, PermissionsExt::from_mode(0o444))
		.unwrap_or_else(|error| panic!("read-only: {error}"));

	let error = run_skill(SkillAction::Install {
		destination: Some(destination),
		force: true,
	})
	.err()
	.unwrap_or_else(|| panic!("expected the read-only install to fail"));

	std::fs::set_permissions(&document, PermissionsExt::from_mode(mode))
		.unwrap_or_else(|error| panic!("restore permissions: {error}"));
	assert!(
		error.to_string().contains("failed to write"),
		"expected a write failure, got: {error}"
	);
}

#[test]
fn install_reports_a_create_failure_when_a_path_component_is_a_file() {
	let directory = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	// A regular file where a directory component belongs makes `create_dir_all`
	// fail, so the failure comes from creating the parent rather than writing.
	let blocker = directory.path().join("blocker");
	fs::write(&blocker, b"not a directory").unwrap_or_else(|error| panic!("blocker: {error}"));

	let error = run_skill(SkillAction::Install {
		destination: Some(blocker.join("monochange")),
		force: false,
	})
	.err()
	.unwrap_or_else(|| panic!("expected the blocked install to fail"));
	assert!(
		error.to_string().contains("failed to create"),
		"unexpected error: {error}"
	);
}
