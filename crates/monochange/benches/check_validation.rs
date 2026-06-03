//! Benchmarks for the `monochange check` / validation pipeline performance.
//!
//! These benchmarks measure the cost of workspace validation, including
//! config loading, versioned-file content validation, and workspace validation.
//! They are designed to catch regressions in the validation phase of `monochange check`,
//! particularly the O(P×G) glob deduplication that was fixed to avoid
//! re-validating the same glob pattern for every package.

use std::fs;
use std::path::Path;

use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;

/// Generate a Dart monorepo fixture with N packages and inherited ecosystem globs.
///
/// This mirrors the structure that caused the O(P×G) glob validation blowup
/// where each package inherits `**/*.pubspec.yaml` from the ecosystem config.
fn generate_dart_inherited_glob_fixture(root: &Path, package_count: usize) {
	use std::fmt::Write;

	let mut config = String::from("[defaults]\nchangelog = false\n\n");
	let _ = writeln!(
		config,
		"[ecosystems.dart]\nversioned_files = [{{ path = \"packages/**/pubspec.yaml\", type = \"dart\" }}]\n"
	);
	for i in 0..package_count {
		let _ = writeln!(
			config,
			"[package.pkg-{i}]\npath = \"packages/pkg-{i}\"\ntype = \"dart\"\n"
		);
	}
	fs::write(root.join("monochange.toml"), config).unwrap();

	for i in 0..package_count {
		let package_dir = root.join(format!("packages/pkg-{i}"));
		fs::create_dir_all(&package_dir).unwrap();
		fs::write(
			package_dir.join("pubspec.yaml"),
			format!("name: pkg_{i}\nversion: 1.0.0\nenvironment:\n  sdk: ^3.0.0\n"),
		)
		.unwrap();
		fs::create_dir_all(package_dir.join("lib")).unwrap();
		fs::write(package_dir.join("lib/pkg.dart"), "// placeholder\n").unwrap();
	}

	// Create a .changeset directory so validation doesn't warn about empty.
	let changeset_dir = root.join(".changeset");
	fs::create_dir_all(&changeset_dir).unwrap();
	fs::write(
		changeset_dir.join("initial.md"),
		"---\npkg-0: patch\n---\n\nInitial change.\n",
	)
	.unwrap();
}

fn bench_validate_versioned_files_with_glob_dedup(c: &mut Criterion) {
	let mut group = c.benchmark_group("validate_versioned_files_glob_dedup");
	group.sample_size(10);

	// Test with increasing numbers of packages to verify the glob dedup
	// keeps validation time sub-linear (ideally constant per unique glob).
	for &package_count in &[10, 50, 100] {
		let label = format!("{package_count}pkg");
		group.bench_with_input(
			BenchmarkId::new("validate_versioned_files", &label),
			&package_count,
			|b, &package_count| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_dart_inherited_glob_fixture(tempdir.path(), package_count);
				// Load config once (this is fast, ~1ms), then benchmark
				// the validation function that previously had O(P×G) blowup.
				let configuration =
					monochange_config::load_workspace_configuration(tempdir.path()).unwrap();
				b.iter(|| {
					monochange_config::validate_versioned_files_content_with_config(
						tempdir.path(),
						&configuration,
					)
					.unwrap()
				});
			},
		);
	}
	group.finish();
}

fn bench_validate_workspace_with_config(c: &mut Criterion) {
	let mut group = c.benchmark_group("validate_workspace_with_config");
	group.sample_size(10);

	for &package_count in &[10, 50, 100] {
		let label = format!("{package_count}pkg");
		group.bench_with_input(
			BenchmarkId::new("validate_workspace", &label),
			&package_count,
			|b, &package_count| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_dart_inherited_glob_fixture(tempdir.path(), package_count);
				let configuration =
					monochange_config::load_workspace_configuration(tempdir.path()).unwrap();
				b.iter(|| {
					monochange_config::validate_workspace_with_config(
						tempdir.path(),
						&configuration,
					)
				});
			},
		);
	}
	group.finish();
}

fn bench_config_load_vs_validate(c: &mut Criterion) {
	let mut group = c.benchmark_group("config_load_vs_validate");
	group.sample_size(10);

	// This benchmark compares config loading time vs validation time
	// to verify that the _with_config variants avoid redundant loads.
	let package_count = 50;
	group.bench_function("load_config_50pkg", |b| {
		let tempdir = tempfile::tempdir().unwrap();
		generate_dart_inherited_glob_fixture(tempdir.path(), package_count);
		b.iter(|| monochange_config::load_workspace_configuration(tempdir.path()).unwrap());
	});

	group.bench_function("validate_with_preloaded_config_50pkg", |b| {
		let tempdir = tempfile::tempdir().unwrap();
		generate_dart_inherited_glob_fixture(tempdir.path(), package_count);
		let configuration =
			monochange_config::load_workspace_configuration(tempdir.path()).unwrap();
		b.iter(|| {
			monochange_config::validate_workspace_with_config(tempdir.path(), &configuration)
		});
	});

	group.bench_function("validate_without_preloaded_config_50pkg", |b| {
		let tempdir = tempfile::tempdir().unwrap();
		generate_dart_inherited_glob_fixture(tempdir.path(), package_count);
		b.iter(|| monochange_config::validate_workspace(tempdir.path()).unwrap());
	});

	group.finish();
}

criterion_group!(
	benches,
	bench_validate_versioned_files_with_glob_dedup,
	bench_validate_workspace_with_config,
	bench_config_load_vs_validate,
);
criterion_main!(benches);
