#!/usr/bin/env node
// Tracks the transferred size of the built website package directory against a
// committed baseline so every pull request that grows the client bundle has to
// acknowledge it, mirroring how scripts/benchmark-phase-budgets.json guards CLI
// runtimes.
//
// Usage:
//   pnpm node scripts/website-size.ts update --site-root app/target/site \
//     [--file scripts/website-size-baseline.json]
//   pnpm node scripts/website-size.ts check --site-root app/target/site \
//     [--file scripts/website-size-baseline.json] [--output comment.md]
//
// `check` fails when the gzip transfer total grows beyond 1%, or when any
// transferred file (`.wasm`/`.js`/`.css`) that existed in the baseline grows
// beyond 5% — WASM codegen varies slightly across build platforms, so the
// per-file guard is looser than the total. Anonymous wasm-split chunks
// (`chunk_N.wasm`) are renumbered between builds, so they share one aggregate
// 5% guard instead of per-file checks. Set
// MONOCHANGE_WEBSITE_SIZE_TOLERANCE (fraction) to adjust the total tolerance.
//
// The baseline predates the dual-frontend split, so its names are flat under
// `pkg/` while both frontends now assemble under `pkg/site/` and
// `pkg/projects/`. Per-file comparisons resolve those renames explicitly (see
// `currentMatches`): a baseline asset that finds no built counterpart fails
// the check, and a name that matches more than one built asset is rejected
// instead of silently resolved. New assets with no baseline entry (the
// projects entry bundle) stay covered by the total guard.
import { readFileSync, readdirSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { gzipSync } from "node:zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..");
const DEFAULT_TOLERANCE = 0.01;
const FILE_TOLERANCE = 0.05;

function die(message) {
	console.error(message);
	process.exit(1);
}

function parseOptions(args, names) {
	const options: Record<string, string> = {};

	for (let index = 0; index < args.length; index += 1) {
		const key = args[index];

		if (!names.includes(key)) die(`unknown argument: ${key}`);
		const value = args[index + 1];

		if (value === undefined) die(`missing value for ${key}`);
		options[key.slice(2).replaceAll("-", "_")] = value;
		index += 1;
	}

	return options;
}

// Both web frontends assemble into one pkg tree, namespaced per entry
// (pkg/site/, pkg/projects/). Files are keyed by their path relative to the
// pkg root so each frontend's artifacts keep their own per-file guard.
// Precompressed `.br`/`.gz` twins are skipped: they are byte-for-byte
// compressions of guarded assets, and CI's gate has always measured the
// uncompressed bundle bytes that browsers actually transfer over the
// compressed connection.
function measure(siteRoot) {
	const pkgDir = join(siteRoot, "pkg");
	let names;
	try {
		names = listFiles(pkgDir, "").filter((name) => !/\.(?:br|gz)$/u.test(name));
	} catch (error) {
		die(`could not read ${pkgDir}: ${error.message}`);
	}
	const files: Record<string, { bytes: number; gzipBytes: number }> = {};
	let totalGzipBytes = 0;

	let totalBytes = 0;

	for (const name of names.toSorted()) {
		const raw = readFileSync(join(pkgDir, name));
		const gzipBytes = gzipSync(raw).byteLength;
		files[name] = { bytes: raw.byteLength, gzipBytes };
		totalBytes += raw.byteLength;
		totalGzipBytes += gzipBytes;
	}

	return { files, totalBytes, totalGzipBytes };
}

function listFiles(dir, prefix) {
	const collected = [];

	for (const entry of readdirSync(dir, { withFileTypes: true })) {
		const relative = prefix.length > 0 ? `${prefix}/${entry.name}` : entry.name;

		if (entry.isDirectory()) {
			collected.push(...listFiles(join(dir, entry.name), relative));
		} else if (entry.isFile()) {
			collected.push(relative);
		}
	}

	return collected;
}

type SiteMeasurement = ReturnType<typeof measure>;

function render(
	baseline: SiteMeasurement,
	current: SiteMeasurement,
	tolerance: number,
	differences: string[],
) {
	const lines = [
		"## Website bundle size",
		"",
		`- Baseline transfer (gzip): ${baseline.totalGzipBytes} bytes`,
		`- Current transfer (gzip): ${current.totalGzipBytes} bytes`,
		`- Raw bytes: ${baseline.totalBytes} → ${current.totalBytes}`,
	];

	if (differences.length > 0) {
		lines.push("", "### Changes beyond tolerance", "");
		lines.push(...differences.map((entry) => `- ${entry}`));
	}

	return lines.join("\n");
}

// wasm-split numbers its shared chunks by position, so any code change can
// renumber them: `chunk_43.wasm` in one build is `chunk_44.wasm` in the next.
// Comparing them by name reports phantom growth, so they are guarded as one
// aggregate instead. Named route splits (`split___<route>_<hash>.wasm`) keep
// stable route stems and stay per-file (across namespaces and hash changes).
const ANONYMOUS_CHUNK = /(?:^|\/)chunk_\d+\.wasm$/u;

function anonymousChunkGzipBytes(measurement: SiteMeasurement): number {
	return Object.entries(measurement.files)
		.filter(([name]) => ANONYMOUS_CHUNK.test(name))
		.reduce((sum, [, file]) => sum + file.gzipBytes, 0);
}

// The baseline is the pre-split bundle: flat names under pkg/, the entry wasm
// named by cargo-leptos (`monochange_app.wasm`). These rules restore per-file
// comparisons across legitimate renames without loosening any budget:
// each baseline asset still has exactly one current counterpart to answer for.
const WEBSITE_NAMESPACE = "site";
const ENTRY_RENAMES: Record<string, string> = {
	// cargo-leptos named the bound module after the output name; wasm-bindgen
	// names the bound module `{output_name}_bg.wasm`.
	"monochange_app.wasm": "monochange_app_bg.wasm",
};
const ROUTE_SPLIT = /^split___(?<stem>[a-z0-9_]+)_\d+\.wasm$/u;
const NAMESPACED_ROUTE_SPLIT = /^(?:[^/]+)\/split___(?<stem>[a-z0-9_]+)_\d+\.wasm$/u;

/**
 * The current files one baseline name may compare against, in preference
 * order: the exact name, the website-namespace move, the entry rename, and —
 * for route splits — every namespace holding that route stem (the hash
 * suffix renumbers between builds, and the projects frontend owns the
 * product routes the combined baseline carried).
 *
 * Callers reject zero matches (a guardable asset cannot vanish) and multiple
 * present matches (ambiguous identity) instead of picking silently.
 */
export function currentMatches(name: string, currentFiles: string[]): string[] {
	const candidates = [name, `${WEBSITE_NAMESPACE}/${name}`];
	const entryRename = ENTRY_RENAMES[name];

	if (entryRename !== undefined) {
		candidates.push(`${WEBSITE_NAMESPACE}/${entryRename}`);
	}

	const routeStem = name.match(ROUTE_SPLIT)?.groups?.stem;

	if (routeStem !== undefined) {
		for (const file of currentFiles) {
			if (NAMESPACED_ROUTE_SPLIT.exec(file)?.groups?.stem === routeStem) {
				candidates.push(file);
			}
		}
	}

	return [...new Set(candidates)];
}

function compare(baseline: SiteMeasurement, current: SiteMeasurement, tolerance: number): string[] {
	const differences = [];
	const grown =
		current.totalGzipBytes - baseline.totalGzipBytes >
		Math.ceil(baseline.totalGzipBytes * tolerance);

	if (grown) {
		differences.push(
			`total gzip transfer grew from ${baseline.totalGzipBytes} to ${current.totalGzipBytes} bytes (tolerance ${(tolerance * 100).toFixed(2)}%)`,
		);
	}

	// WASM codegen differs slightly between host platforms, so per-file
	// enforcement applies only to the files browsers actually transfer and
	// allows wider variance than the total; the total stays the tight guard.
	const perFileTolerance = Math.max(tolerance, FILE_TOLERANCE);
	const exceeds = (before: number, after: number) =>
		after - before > Math.ceil(before * perFileTolerance);
	const percent = `${(perFileTolerance * 100).toFixed(2)}%`;
	const currentNames = Object.keys(current.files);

	for (const [name, before] of Object.entries(baseline.files)) {
		if (!/\.(?:wasm|js|css)$/u.test(name) || ANONYMOUS_CHUNK.test(name)) continue;
		const present = currentMatches(name, currentNames).filter((candidate) =>
			Object.hasOwn(current.files, candidate),
		);

		if (present.length === 0) {
			differences.push(
				`${name}: no built asset matches this baseline entry; a renamed or removed asset cannot leave its allowance behind`,
			);

			continue;
		}

		if (present.length > 1) {
			die(
				`ambiguous size comparison for ${name}: matches ${present.join(", ")}; make the artifact names unambiguous or extend the rename tables`,
			);
		}

		const currentName = present[0];
		const after = current.files[currentName];

		if (exceeds(before.gzipBytes, after.gzipBytes)) {
			differences.push(
				`${name} → ${currentName} gzip grew from ${before.gzipBytes} to ${after.gzipBytes} bytes (tolerance ${percent})`,
			);
		}
	}

	const chunksBefore = anonymousChunkGzipBytes(baseline);
	const chunksAfter = anonymousChunkGzipBytes(current);

	if (exceeds(chunksBefore, chunksAfter)) {
		differences.push(
			`anonymous wasm-split chunks (chunk_N.wasm) gzip grew from ${chunksBefore} to ${chunksAfter} bytes (tolerance ${percent})`,
		);
	}

	return differences;
}

function main() {
	const [command, ...rest] = process.argv.slice(2);

	if (command !== "check" && command !== "update") {
		die(
			"usage: website-size.ts <check|update> --site-root <dir> [--file <baseline>] [--output <md>]",
		);
	}

	const options = parseOptions(rest, ["--site-root", "--file", "--output"]);
	const siteRoot = resolve(repoRoot, options.site_root ?? "app/target/site");
	const file = options.file ?? join(scriptDir, "website-size-baseline.json");
	const current = measure(siteRoot);

	if (command === "update") {
		writeFileSync(file, `${JSON.stringify(current, null, "\t")}\n`);
		console.log(`baseline updated: ${file} (${current.totalGzipBytes} gzip bytes)`);

		return;
	}

	let baseline: ReturnType<typeof measure>;
	try {
		baseline = JSON.parse(readFileSync(file, "utf8"));
	} catch (error) {
		die(`could not read baseline ${file}: ${error.message}; run the update command first`);
	}
	const tolerance = Number.parseFloat(process.env.MONOCHANGE_WEBSITE_SIZE_TOLERANCE ?? "");
	const toleranceValue = Number.isFinite(tolerance) ? tolerance : DEFAULT_TOLERANCE;
	const differences = compare(baseline, current, toleranceValue);

	const report = render(baseline, current, toleranceValue, differences);
	console.log(report);

	if (options.output) {
		writeFileSync(options.output, `${report}\n`);
	}

	if (differences.length > 0) {
		die(
			"\nBundle growth beyond tolerance. If the growth is deliberate, rerun the update command in the same pull request and explain the trade-off.",
		);
	}
}

// Run the CLI only when this file is the entry module, so tests can import
// the pure comparison helpers. The entry path is canonicalized because the
// ESM loader resolves import.meta.url through real paths (macOS /var →
// /private/var), while argv[1] keeps the invocation path.
const entryPath = process.argv[1] ? pathToFileURL(realpathSync(process.argv[1])).href : undefined;

if (entryPath !== undefined && import.meta.url === entryPath) {
	main();
}
