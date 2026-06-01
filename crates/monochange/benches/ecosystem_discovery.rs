//! Benchmarks for ecosystem package discovery performance.
//!
//! These benchmarks measure discovery in generated monorepo fixtures of varying
//! sizes. They are designed to catch regressions in the discovery phase of
//! `mc init`, `mc step:discover`, `mc release`, and mixed-ecosystem workflows.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use monochange_core::EcosystemRegistry;

fn fixture_path(name: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures")
		.join(name)
}

fn write_file(path: &Path, content: impl AsRef<str>) {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).unwrap();
	}
	fs::write(path, content.as_ref()).unwrap();
}

fn generate_npm_fixture(root: &Path, package_count: usize) {
	write_file(
		&root.join("package.json"),
		"{\"private\":true,\"workspaces\":[\"packages/*\"]}\n",
	);
	for index in 0..package_count {
		write_file(
			&root.join(format!("packages/pkg-{index}/package.json")),
			format!(
				"{{\"name\":\"@bench/pkg-{index}\",\"version\":\"1.0.0\",\"private\":false}}\n"
			),
		);
	}
}

fn generate_deno_fixture(root: &Path, package_count: usize) {
	write_file(
		&root.join("deno.json"),
		"{\"workspace\":[\"packages/*\"]}\n",
	);
	for index in 0..package_count {
		write_file(
			&root.join(format!("packages/pkg-{index}/deno.json")),
			format!("{{\"name\":\"@bench/pkg-{index}\",\"version\":\"1.0.0\"}}\n"),
		);
	}
}

fn generate_python_fixture(root: &Path, package_count: usize) {
	write_file(
		&root.join("pyproject.toml"),
		"[tool.uv.workspace]\nmembers = [\"packages/*\"]\n",
	);
	for index in 0..package_count {
		write_file(
			&root.join(format!("packages/pkg-{index}/pyproject.toml")),
			format!("[project]\nname = \"bench-pkg-{index}\"\nversion = \"1.0.0\"\n"),
		);
	}
}

fn generate_go_fixture(root: &Path, package_count: usize) {
	for index in 0..package_count {
		write_file(
			&root.join(format!("packages/pkg-{index}/go.mod")),
			format!("module example.com/bench/pkg-{index}\n\ngo 1.22\n"),
		);
	}
}

fn generate_mixed_fixture(root: &Path, package_count: usize) {
	generate_npm_fixture(&root.join("npm"), package_count);
	generate_deno_fixture(&root.join("deno"), package_count);
	generate_python_fixture(&root.join("python"), package_count);
	generate_go_fixture(&root.join("go"), package_count);
}

fn bench_generated_discovery(c: &mut Criterion) {
	let mut group = c.benchmark_group("generated_ecosystem_discovery");
	group.sample_size(10);

	for &package_count in &[50, 100, 500] {
		let label = format!("{package_count}_packages");
		group.bench_with_input(
			BenchmarkId::new("npm", &label),
			&package_count,
			|b, &count| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_npm_fixture(tempdir.path(), count);
				b.iter(|| monochange_npm::discover_npm_packages(tempdir.path()).unwrap());
			},
		);
		group.bench_with_input(
			BenchmarkId::new("deno", &label),
			&package_count,
			|b, &count| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_deno_fixture(tempdir.path(), count);
				b.iter(|| monochange_deno::discover_deno_packages(tempdir.path()).unwrap());
			},
		);
		group.bench_with_input(
			BenchmarkId::new("python", &label),
			&package_count,
			|b, &count| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_python_fixture(tempdir.path(), count);
				b.iter(|| monochange_python::discover_python_packages(tempdir.path()).unwrap());
			},
		);
		group.bench_with_input(
			BenchmarkId::new("go", &label),
			&package_count,
			|b, &count| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_go_fixture(tempdir.path(), count);
				b.iter(|| monochange_go::discover_go_modules(tempdir.path()).unwrap());
			},
		);
		group.bench_with_input(
			BenchmarkId::new("mixed", &label),
			&package_count,
			|b, &count| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_mixed_fixture(tempdir.path(), count);
				let registry = EcosystemRegistry::new()
					.with_adapter(Box::new(monochange_npm::NpmAdapter))
					.with_adapter(Box::new(monochange_deno::DenoAdapter))
					.with_adapter(Box::new(monochange_python::PythonAdapter))
					.with_adapter(Box::new(monochange_go::GoAdapter));
				b.iter(|| registry.discover_all(tempdir.path()).unwrap());
			},
		);
	}

	group.finish();
}

fn bench_existing_fixture_discovery(c: &mut Criterion) {
	let mut group = c.benchmark_group("existing_fixture_discovery");

	let dart_small = fixture_path("dart/workspace");
	if dart_small.exists() {
		group.bench_with_input(
			BenchmarkId::new("dart", "2_packages"),
			&dart_small,
			|b, root| {
				b.iter(|| monochange_dart::discover_dart_packages(root).unwrap());
			},
		);
	}

	let dart_medium = fixture_path("dart/monorepo");
	if dart_medium.exists() {
		group.bench_with_input(
			BenchmarkId::new("dart", "11_packages"),
			&dart_medium,
			|b, root| {
				b.iter(|| monochange_dart::discover_dart_packages(root).unwrap());
			},
		);
	}

	let dart_large = fixture_path("dart/large-monorepo");
	if dart_large.exists() {
		group.bench_with_input(
			BenchmarkId::new("dart", "51_packages"),
			&dart_large,
			|b, root| {
				b.iter(|| monochange_dart::discover_dart_packages(root).unwrap());
			},
		);
	}

	let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	group.bench_with_input(
		BenchmarkId::new("cargo", "workspace"),
		&repo_root,
		|b, root| {
			b.iter(|| monochange_cargo::discover_cargo_packages(root).unwrap());
		},
	);

	group.finish();
}

criterion_group!(
	benches,
	bench_generated_discovery,
	bench_existing_fixture_discovery
);
criterion_main!(benches);
