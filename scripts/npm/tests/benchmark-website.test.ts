import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, test } from "vitest";

import { EAGER_WASM_ENTRIES, checkAssetBudgets, listSiteFiles } from "../../benchmark-website.ts";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

/** Writes a pkg tree; every name maps to a byte length of compressible input. */
function writeSite(tag: string, files: Record<string, number>): string {
	const root = mkdtempSync(join(tmpdir(), `benchmark-website-${tag}-`));

	for (const [name, size] of Object.entries(files)) {
		const path = join(root, "pkg", name);
		mkdirSync(dirname(path), { recursive: true });
		writeFileSync(path, Buffer.alloc(size, 0x61));
	}

	return root;
}

const budgets = (assets: Record<string, number>) => ({ assets });

describe("benchmark-website asset accounting", () => {
	const roots: string[] = [];

	afterEach(() => {
		for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
	});

	function site(tag: string, files: Record<string, number>): string {
		const root = writeSite(tag, files);
		roots.push(root);

		return root;
	}

	test("excludes precompressed twins from every raw total", () => {
		const root = site("twins", {
			"site/monochange_app_bg.wasm": 1_000,
			"site/monochange_app_bg.wasm.br": 500,
			"site/monochange_app_bg.wasm.gz": 400,
			"site/monochange_app.js": 600,
			"site/monochange_app.js.br": 300,
		});
		const files = listSiteFiles(root);
		const violations: string[] = [];
		const totals = checkAssetBudgets(
			budgets({ pkgTotalBytes: 2_000, pkgWasmBytes: 1_500, pkgJsBytes: 1_000 }),
			files,
			violations,
		);

		assert.deepEqual(files.map((file) => file.name).toSorted(), [
			"site/monochange_app.js",
			"site/monochange_app_bg.wasm",
		]);
		assert.equal(totals.totalBytes, 1_600);
		assert.equal(totals.wasmBytes, 1_000);
		assert.equal(totals.jsBytes, 600);
		assert.deepEqual(violations, [
			"missing eager wasm entry bundle(s): projects/monochange_projects_web_bg.wasm",
		]);
	});

	test("counts nested namespaces in the raw totals", () => {
		const root = site("nested", {
			"site/monochange_app_bg.wasm": 1_000,
			"projects/monochange_projects_web_bg.wasm": 800,
			"projects/chunk_5.wasm": 200,
		});
		const files = listSiteFiles(root);
		const violations: string[] = [];
		const totals = checkAssetBudgets(budgets({}), files, violations);

		assert.equal(totals.wasmBytes, 2_000);
		assert.equal(totals.totalBytes, 2_000);
		// No asset budgets are set beyond the eager entries, which both fit.
		assert.deepEqual(violations, []);
	});

	test("fails when any raw total exceeds its budget", () => {
		const root = site("over-budget", {
			"site/monochange_app_bg.wasm": 2_000,
			"site/monochange_app.js": 2_000,
			"projects/monochange_projects_web_bg.wasm": 500,
		});
		const violations: string[] = [];
		checkAssetBudgets(
			budgets({ pkgTotalBytes: 4_000, pkgWasmBytes: 2_000, pkgJsBytes: 1_000 }),
			listSiteFiles(root),
			violations,
		);

		assert.deepEqual(violations, [
			"pkgTotalBytes 4500 exceeds budget 4000",
			"pkgWasmBytes 2500 exceeds budget 2000",
			"pkgJsBytes 2000 exceeds budget 1000",
		]);
	});

	test("checks every real eager wasm entry against the eager budget", () => {
		const root = site("eager", {
			"site/monochange_app_bg.wasm": 2_100_000,
			"projects/monochange_projects_web_bg.wasm": 2_300_000,
		});
		const violations: string[] = [];
		checkAssetBudgets(budgets({ pkgEagerWasmBytes: 2_200_000 }), listSiteFiles(root), violations);

		// Each entry must fit on its own: the projects entry busts the limit
		// even though the website entry fits.
		assert.deepEqual(violations, [
			"pkgEagerWasmBytes projects/monochange_projects_web_bg.wasm 2300000 exceeds budget 2200000",
		]);
	});

	test("the eager entry names match both frontend namespaces", () => {
		assert.deepEqual(EAGER_WASM_ENTRIES, [
			"site/monochange_app_bg.wasm",
			"projects/monochange_projects_web_bg.wasm",
		]);
	});

	test("missing pkg directory fails loudly", () => {
		const root = mkdtempSync(join(tmpdir(), "benchmark-website-missing-"));
		roots.push(root);
		const result = spawnSync(
			process.execPath,
			[join(repoRoot, "scripts/benchmark-website.ts"), "run", "--site-root", root],
			{ encoding: "utf8" },
		);

		assert.notEqual(result.status, 0);
		assert.match(result.stderr, /missing .*pkg; build both frontends first/);
	});
});
