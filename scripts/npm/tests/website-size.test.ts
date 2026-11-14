import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { afterEach, describe, test } from "vitest";

import { currentMatches } from "../../website-size.ts";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const scriptPath = join(repoRoot, "scripts/website-size.ts");
const loaderName = "__wasm_split.______________________.js";

type FileEntry = { bytes: number; gzipBytes: number };

/** Incompressible filler (xorshift stream), so gzip grows with content
 * instead of a repeating pattern collapsing each file to a constant. */
function filler(size: number): Buffer {
	let state = 0x9e3779b9;
	const raw = Buffer.allocUnsafe(size);
	for (let index = 0; index < size; index += 1) {
		state ^= state << 13;
		state >>>= 0;
		state ^= state >>> 17;
		state ^= state << 5;
		state >>>= 0;
		raw[index] = state & 0xff;
	}
	return raw;
}

function gzipOf(size: number): number {
	return gzipSync(filler(size)).byteLength;
}

/** Builds a measurement object the way `measure` would, for fixture baselines. */
function baselineFromFiles(files: Record<string, number>): {
	files: Record<string, FileEntry>;
	totalBytes: number;
	totalGzipBytes: number;
} {
	const measured: Record<string, FileEntry> = {};
	let totalBytes = 0;
	let totalGzipBytes = 0;

	for (const [name, size] of Object.entries(files)) {
		const gzipBytes = gzipOf(size);
		measured[name] = { bytes: size, gzipBytes };
		totalBytes += size;
		totalGzipBytes += gzipBytes;
	}

	return { files: measured, totalBytes, totalGzipBytes };
}

// The pre-split baseline: flat names, the cargo-leptos entry wasm name, the
// auth-callback route stem, and the combined bundle's chunk allowance. The
// entry wasm (and chunk_11) are sized so the split bundle's totals — the
// site entry plus the whole new projects namespace — stay inside the 1%
// total tolerance, as the real budgets require.
function flatBaselineSizes(): Record<string, number> {
	return {
		[loaderName]: 3_000,
		"__wasm_split_manifest.json": 1_400,
		"monochange_app.css": 8_000,
		"monochange_app.js": 27_000,
		"monochange_app.wasm": 322_000,
		"split___auth_callback_route_view_17242783178260986175.wasm": 5_000,
		"split___organization_route_view_12043108128510937669.wasm": 9_000,
		"chunk_11.wasm": 3_000,
	};
}

function namespacedCurrentSizes(): Record<string, number> {
	return {
		[`site/${loaderName}`]: 3_000,
		"site/__wasm_split_manifest.json": 1_400,
		"site/monochange_app.css": 8_000,
		"site/monochange_app.js": 27_000,
		"site/monochange_app_bg.wasm": 200_000,
		"site/split___auth_callback_route_view_91.wasm": 5_000,
		"site/split___pricing_route_view_92.wasm": 4_000,
		"site/chunk_11.wasm": 900,
		"projects/monochange_projects_web.js": 26_000,
		"projects/monochange_projects_web_bg.wasm": 90_000,
		"projects/split___organization_route_view_93.wasm": 9_000,
		"projects/chunk_5.wasm": 1_500,
	};
}

/** Writes a pkg tree where every file is `size` bytes of compressible input. */
function writeSite(tag: string, files: Record<string, number>): { root: string; baseline: string } {
	const root = mkdtempSync(join(tmpdir(), `website-size-${tag}-`));
	for (const [name, size] of Object.entries(files)) {
		const path = join(root, "pkg", name);
		mkdirSync(dirname(path), { recursive: true });
		writeFileSync(path, filler(size));
	}
	const baseline = join(root, "baseline.json");

	return { root, baseline };
}

function runCheck(siteRoot: string, baselineFile: string) {
	return spawnSync(
		process.execPath,
		[scriptPath, "check", "--site-root", siteRoot, "--file", baselineFile],
		{ encoding: "utf8" },
	);
}

describe("website-size currentMatches", () => {
	test("matches renumbered routes only within their entry namespace", () => {
		assert.deepEqual(
			currentMatches("projects/split___portal_route_view_123.wasm", [
				"site/split___portal_route_view_456.wasm",
				"projects/split___portal_route_view_789.wasm",
			]),
			["projects/split___portal_route_view_789.wasm"],
		);
	});
	test("resolves the namespace move, the entry wasm rename, and exact names", () => {
		const current = [
			"site/monochange_app.js",
			"site/monochange_app_bg.wasm",
			"site/monochange_app.css",
			`site/${loaderName}`,
		];

		assert.deepEqual(currentMatches("monochange_app.js", current), [
			"monochange_app.js",
			"site/monochange_app.js",
		]);
		assert.deepEqual(currentMatches("monochange_app.wasm", current), [
			"monochange_app.wasm",
			"site/monochange_app.wasm",
			"site/monochange_app_bg.wasm",
		]);
	});

	test("resolves renumbered and cross-namespace lazy route splits by route stem", () => {
		const current = [
			"site/split___auth_callback_route_view_91.wasm",
			"projects/split___organization_route_view_93.wasm",
		];

		assert.deepEqual(
			currentMatches("split___auth_callback_route_view_17242783178260986175.wasm", current),
			[
				"split___auth_callback_route_view_17242783178260986175.wasm",
				"site/split___auth_callback_route_view_17242783178260986175.wasm",
				"site/split___auth_callback_route_view_91.wasm",
			],
		);
		assert.deepEqual(
			currentMatches("split___organization_route_view_12043108128510937669.wasm", current).at(-1),
			"projects/split___organization_route_view_93.wasm",
		);
	});

	test("exposes ambiguity instead of resolving it", () => {
		const current = [
			"site/split___dashboard_route_view_1.wasm",
			"projects/split___dashboard_route_view_2.wasm",
		];
		const matches = currentMatches(
			"split___dashboard_route_view_14578883471362314054.wasm",
			current,
		);

		assert.equal(matches.filter((name) => current.includes(name)).length, 2);
	});
});

describe("website-size check", () => {
	const roots: string[] = [];

	afterEach(() => {
		for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
	});

	test("guards deployed fonts even when the pkg total is otherwise unchanged", () => {
		const site = writeSite("fonts", { "site/monochange_app.js": 10000 });
		roots.push(site.root);
		mkdirSync(join(site.root, "fonts"));
		writeFileSync(join(site.root, "fonts", "face.woff2"), filler(1000));
		const initial = spawnSync(
			process.execPath,
			[scriptPath, "update", "--site-root", site.root, "--file", site.baseline],
			{ encoding: "utf8" },
		);
		assert.equal(initial.status, 0, initial.stderr);
		writeFileSync(join(site.root, "fonts", "face.woff2"), filler(1100));
		const result = runCheck(site.root, site.baseline);
		assert.notEqual(result.status, 0);
		assert.match(result.stdout, /fonts\/face\.woff2.*gzip grew/);
	});

	test("guards each entry's shared chunks independently", () => {
		const sizes = { "site/chunk_1.wasm": 1000, "projects/chunk_1.wasm": 1000 };
		const { root, baseline } = writeScenario(
			"separate-chunks",
			{ "site/chunk_2.wasm": 1200, "projects/chunk_3.wasm": 800 },
			sizes,
		);
		const result = runCheck(root, baseline);
		assert.notEqual(result.status, 0);
		assert.match(result.stdout, /anonymous wasm-split chunks .*site gzip grew/);
	});

	function writeScenario(
		tag: string,
		current: Record<string, number>,
		baselineSizes: Record<string, number>,
		/** Grow the baseline entry wasm until its gzip total covers the
		 * current site's, mirroring the real budgets where the split bundle
		 * must fit the combined bundle's transfer allowance. */
		balanceTotal = false,
	) {
		const scenario = writeSite(tag, current);
		roots.push(scenario.root);
		let baseline = baselineFromFiles(baselineSizes);
		const currentTotal = Object.values(current).reduce((sum, size) => sum + gzipOf(size), 0);
		while (balanceTotal && baseline.totalGzipBytes < currentTotal) {
			baselineSizes["monochange_app.wasm"] += 4_096;
			baseline = baselineFromFiles(baselineSizes);
		}
		writeFileSync(scenario.baseline, `${JSON.stringify(baseline, null, "\t")}\n`);

		return scenario;
	}

	test("passes per-file guards through the namespace and entry renames", () => {
		const { root, baseline: baselineFile } = writeScenario(
			"renames-pass",
			namespacedCurrentSizes(),
			flatBaselineSizes(),
			true,
		);
		const result = runCheck(root, baselineFile);

		assert.equal(result.status, 0, result.stdout);
	});

	test("fails when the renamed entry wasm exceeds its old allowance", () => {
		const current = namespacedCurrentSizes();
		current["site/monochange_app_bg.wasm"] = 400_000;
		const { root, baseline: baselineFile } = writeScenario(
			"entry-grows",
			current,
			flatBaselineSizes(),
		);
		const result = runCheck(root, baselineFile);

		assert.notEqual(result.status, 0);
		assert.match(result.stdout, /monochange_app\.wasm → site\/monochange_app_bg\.wasm gzip grew/);
	});

	test("fails when a renumbered lazy route split exceeds its old allowance", () => {
		const current = namespacedCurrentSizes();
		current["site/split___auth_callback_route_view_91.wasm"] = 50_000;
		const { root, baseline: baselineFile } = writeScenario(
			"split-grows",
			current,
			flatBaselineSizes(),
		);
		const result = runCheck(root, baselineFile);

		assert.notEqual(result.status, 0);
		assert.match(
			result.stdout,
			/auth_callback_route_view[\s\S]*auth_callback_route_view_91\.wasm gzip grew/,
		);
	});

	test("fails when a cross-namespace split exceeds its old allowance", () => {
		const current = namespacedCurrentSizes();
		current["projects/split___organization_route_view_93.wasm"] = 90_000;
		const { root, baseline: baselineFile } = writeScenario(
			"split-cross",
			current,
			flatBaselineSizes(),
		);
		const result = runCheck(root, baselineFile);

		assert.notEqual(result.status, 0);
		assert.match(result.stdout, /→ projects\/split___organization_route_view_93\.wasm gzip grew/);
	});

	test("fails when a baseline asset has no built counterpart", () => {
		const current = namespacedCurrentSizes();
		delete current["site/monochange_app.css"];
		const { root, baseline: baselineFile } = writeScenario(
			"asset-vanishes",
			current,
			flatBaselineSizes(),
		);
		const result = runCheck(root, baselineFile);

		assert.notEqual(result.status, 0);
		assert.match(result.stdout, /monochange_app\.css: no built asset matches/);
	});

	test("rejects an ambiguous split match instead of picking one", () => {
		const baselineSizes = flatBaselineSizes();
		baselineSizes["split___dashboard_route_view_14578883471362314054.wasm"] = 5_000;
		const current = namespacedCurrentSizes();
		current["site/split___dashboard_route_view_1.wasm"] = 5_000;
		current["projects/split___dashboard_route_view_2.wasm"] = 5_000;
		const { root, baseline: baselineFile } = writeScenario("ambiguous", current, baselineSizes);
		const result = runCheck(root, baselineFile);

		assert.notEqual(result.status, 0);
		assert.match(result.stderr, /ambiguous size comparison for split___dashboard_route_view/);
	});

	test("guards anonymous chunks as one aggregate across namespaces", () => {
		const current = namespacedCurrentSizes();
		current["projects/chunk_5.wasm"] = 40_000;
		const { root, baseline: baselineFile } = writeScenario(
			"chunks-aggregate",
			current,
			flatBaselineSizes(),
		);
		const result = runCheck(root, baselineFile);

		assert.notEqual(result.status, 0);
		assert.match(result.stdout, /anonymous wasm-split chunks \(chunk_N\.wasm\) gzip grew/);
	});

	test("holds new namespaces to the unchanged total", () => {
		const current = namespacedCurrentSizes();
		current["projects/monochange_projects_web_bg.wasm"] = 2_000_000;
		const { root, baseline: baselineFile } = writeScenario(
			"total-grows",
			current,
			flatBaselineSizes(),
		);
		const result = runCheck(root, baselineFile);

		assert.notEqual(result.status, 0);
		assert.match(result.stdout, /total gzip transfer grew/);
	});
});
