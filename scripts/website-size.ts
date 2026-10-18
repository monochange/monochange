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
// `check` fails when the gzip transfer total, or any single file that existed
// in the baseline, grows beyond the allowed headroom. Set
// MONOCHANGE_WEBSITE_SIZE_TOLERANCE (fraction, default 0.01) to adjust.

import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..");
const DEFAULT_TOLERANCE = 0.01;

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

function measure(siteRoot) {
	const pkgDir = join(siteRoot, "pkg");
	let names;
	try {
		names = readdirSync(pkgDir);
	} catch (error) {
		die(`could not read ${pkgDir}: ${error.message}`);
	}
	const files: Record<string, { bytes: number; gzipBytes: number }> = {};
	let totalGzipBytes = 0;
	let totalBytes = 0;
	for (const name of names.filter((entry) => statSync(join(pkgDir, entry)).isFile()).toSorted()) {
		const raw = readFileSync(join(pkgDir, name));
		const gzipBytes = gzipSync(raw).byteLength;
		files[name] = { bytes: raw.byteLength, gzipBytes };
		totalBytes += raw.byteLength;
		totalGzipBytes += gzipBytes;
	}
	return { files, totalBytes, totalGzipBytes };
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
	for (const [name, before] of Object.entries(baseline.files)) {
		const after = current.files[name];
		if (!after) continue;
		if (after.gzipBytes - before.gzipBytes > Math.ceil(before.gzipBytes * tolerance)) {
			differences.push(`${name} gzip grew from ${before.gzipBytes} to ${after.gzipBytes} bytes`);
		}
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

main();
