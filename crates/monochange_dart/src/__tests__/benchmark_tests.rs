//! Benchmarks for Dart package discovery performance.

use std::path::Path;
use std::time::Instant;

use crate::discover_dart_packages;

#[test]
fn discover_dart_packages_large_monorepo_benchmark() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dart/large-monorepo");

	// Warm up
	let _ = discover_dart_packages(&fixture_root).unwrap();

	// Benchmark
	let start = Instant::now();
	let iterations = 10;
	for _ in 0..iterations {
		let discovery = discover_dart_packages(&fixture_root).unwrap();
		assert_eq!(
			discovery.packages.len(),
			51,
			"should discover all 50 packages plus root"
		);
	}
	let elapsed = start.elapsed();
	let avg_per_iter = elapsed / iterations;

	println!("Dart discovery (50 packages): {elapsed:?} total, {avg_per_iter:?} per iteration");

	// The optimization should keep this well under 100ms per iteration
	// even on CI. Before the fix, each iteration would do 2 full WalkDir
	// traversals instead of 1.
	assert!(
		avg_per_iter.as_millis() < 500,
		"discovery should be fast, took {avg_per_iter:?} per iteration"
	);
}
