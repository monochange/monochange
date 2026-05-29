//! Benchmarks for ecosystem package discovery performance.
//!
//! These benchmarks measure the time to discover packages in monorepo fixtures
//! of varying sizes. They are designed to catch regressions in the discovery
//! phase of `mc release` and `mc discover`.

use std::path::Path;

use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;

fn fixture_path(name: &str) -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures")
		.join(name)
}

fn bench_dart_discovery(c: &mut Criterion) {
	let mut group = c.benchmark_group("dart_discovery");

	// Small fixture: 2 packages
	let small = fixture_path("dart/workspace");
	if small.exists() {
		group.bench_with_input(
			BenchmarkId::new("discover", "2_packages"),
			&small,
			|b, root| {
				b.iter(|| monochange_dart::discover_dart_packages(root).unwrap());
			},
		);
	}

	// Medium fixture: 11 packages
	let medium = fixture_path("dart/monorepo");
	if medium.exists() {
		group.bench_with_input(
			BenchmarkId::new("discover", "11_packages"),
			&medium,
			|b, root| {
				b.iter(|| monochange_dart::discover_dart_packages(root).unwrap());
			},
		);
	}

	// Large fixture: 51 packages
	let large = fixture_path("dart/large-monorepo");
	if large.exists() {
		group.bench_with_input(
			BenchmarkId::new("discover", "51_packages"),
			&large,
			|b, root| {
				b.iter(|| monochange_dart::discover_dart_packages(root).unwrap());
			},
		);
	}

	group.finish();
}

fn bench_cargo_discovery(c: &mut Criterion) {
	let mut group = c.benchmark_group("cargo_discovery");

	// Use the monochange repo itself as a benchmark
	let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	group.bench_with_input(
		BenchmarkId::new("discover", "workspace"),
		&repo_root,
		|b, root| {
			b.iter(|| monochange_cargo::discover_cargo_packages(root).unwrap());
		},
	);

	group.finish();
}

criterion_group!(benches, bench_dart_discovery, bench_cargo_discovery);
criterion_main!(benches);
