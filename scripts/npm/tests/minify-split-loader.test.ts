import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
	chmodSync,
	cpSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, test } from "vitest";

import { minifyEntryGlue } from "../../minify-split-loader.ts";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const scriptPath = join(repoRoot, "scripts/minify-split-loader.ts");
const fixtureRoot = join(repoRoot, "fixtures/tests/website-split-loader-minify");

const loaderName = "__wasm_split.______________________.js";
const bookExport = "__wasm_split_load___book_route_view_17666993872358248534";
const pricingExport = "__wasm_split_load___pricing_route_view_14841084612111332454";

function createSiteRoot(scenario: string): string {
	const root = mkdtempSync(join(tmpdir(), "minify-split-loader-"));
	cpSync(join(fixtureRoot, scenario), root, { recursive: true });

	return root;
}

function runMinify(siteRoot: string, env: Record<string, string> = {}) {
	return spawnSync(process.execPath, [scriptPath, "--site-root", siteRoot], {
		encoding: "utf8",
		cwd: repoRoot,
		env: { ...process.env, ...env },
	});
}

function readLoader(siteRoot: string): string {
	return readFileSync(join(siteRoot, "pkg", loaderName), "utf8");
}

describe("minify-split-loader", () => {
	const roots: string[] = [];

	afterEach(() => {
		for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
	});

	function scenario(scenarioName: string): string {
		const root = createSiteRoot(scenarioName);
		roots.push(root);
		return root;
	}

	test("minifies the loader and preserves the split contract", () => {
		const siteRoot = scenario("loader-site");
		const fixtureLoader = readFileSync(join(fixtureRoot, "loader-site/pkg", loaderName));
		const fixtureManifest = readFileSync(
			join(fixtureRoot, "loader-site/pkg/__wasm_split_manifest.json"),
		);
		const fixtureGlue = readFileSync(join(fixtureRoot, "loader-site/pkg/monochange_app.js"));

		const result = runMinify(siteRoot);
		assert.equal(result.status, 0, result.stderr || result.stdout);
		// Progress goes to stderr so stdout can carry structured output (the
		// leptoskit plugin protocol).
		assert.match(result.stderr, new RegExp(`${loaderName} gzip \\d+ → \\d+ bytes`));
		assert.match(result.stderr, /minified 1 split loader\(s\); gzip \d+ → \d+ bytes/);

		const minified = readLoader(siteRoot);
		assert.ok(
			Buffer.byteLength(minified, "utf8") < fixtureLoader.byteLength,
			"minified loader must shrink",
		);
		// Route-name comments and helper identifiers disappear.
		assert.ok(!minified.includes("/* __book_route_view"), "route comments must be stripped");
		assert.ok(!minified.includes("makeLoad"), "helper identifiers must be mangled");
		assert.ok(!minified.includes("sharedImports"), "helper identifiers must be mangled");
		// The hydrated glue imports these exact names.
		assert.ok(minified.includes(bookExport), "book route initializer must survive");
		assert.ok(minified.includes(pricingExport), "pricing route initializer must survive");
		// Runtime references stay byte-identical.
		assert.ok(
			minified.includes('"/pkg/monochange_app.js"'),
			"shared imports module must be preserved",
		);
		assert.ok(minified.includes('"./chunk_11.wasm"'), "chunk URL must be preserved");
		assert.ok(minified.includes('"./chunk_12.wasm"'), "chunk URL must be preserved");
		assert.ok(minified.includes('"./split___book_route_view_17666993872358248534.wasm"'));
		assert.ok(minified.includes('"./split___pricing_route_view_14841084612111332454.wasm"'));
		assert.ok(minified.includes("import.meta.url"), "relative URL resolution must be preserved");
		// Neighbors are untouched.
		assert.deepEqual(
			readFileSync(join(siteRoot, "pkg/__wasm_split_manifest.json")),
			fixtureManifest,
			"manifest must stay untouched",
		);
		assert.deepEqual(readFileSync(join(siteRoot, "pkg/monochange_app.js")), fixtureGlue);
	});

	test("minification is idempotent", () => {
		const siteRoot = scenario("loader-site");
		const first = runMinify(siteRoot);
		assert.equal(first.status, 0, first.stderr || first.stdout);
		const afterFirst = readFileSync(join(siteRoot, "pkg", loaderName));

		const second = runMinify(siteRoot);
		assert.equal(second.status, 0, second.stderr || second.stdout);
		// esbuild mangles identifiers differently per input shape, so the second
		// pass is stable in size and contract, not byte-identical.
		assert.equal(readFileSync(join(siteRoot, "pkg", loaderName)).byteLength, afterFirst.byteLength);
		const minified = readLoader(siteRoot);
		assert.ok(minified.includes(bookExport));
		assert.ok(minified.includes(pricingExport));
	});

	test("minifies from any working directory like the Docker build does", () => {
		const root = mkdtempSync(join(tmpdir(), "docker-split-loader-"));
		roots.push(root);
		mkdirSync(join(root, "scripts"));
		const copiedScript = join(root, "scripts/minify-split-loader.ts");
		cpSync(scriptPath, copiedScript);
		const siteRoot = join(root, "app/target/site");
		cpSync(join(fixtureRoot, "loader-site"), siteRoot, { recursive: true });
		const result = spawnSync(process.execPath, [copiedScript, "--site-root", "app/target/site"], {
			cwd: join(root, "app"),
			encoding: "utf8",
			env: {
				...process.env,
				MONOCHANGE_ESBUILD_BIN: join(repoRoot, "node_modules/.bin/esbuild"),
			},
		});
		assert.equal(result.status, 0, result.stderr || result.stdout);
		assert.ok(readLoader(siteRoot).includes(bookExport));
		assert.ok(!readLoader(siteRoot).includes("makeLoad"));
	});

	test("the Docker build minifies through the leptoskit plugin", () => {
		// The release image runs the guarded minifier inside `leptoskit build`
		// via the tooling plugin; a direct post-build invocation would
		// re-minify after precompression and desync the .br/.gz twins.
		const dockerfile = readFileSync(join(repoRoot, "Dockerfile"), "utf8");
		assert.ok(
			dockerfile.includes("COPY scripts/leptoskit-plugin/ scripts/leptoskit-plugin/"),
			"Docker must ship the leptoskit tooling plugin",
		);
		assert.ok(
			dockerfile.includes("/usr/local/bin/leptoskit-plugin-monochange-web-assets"),
			"Docker must put the plugin on PATH by its default leptoskit name",
		);
		assert.ok(
			!/node [^\n]*minify-split-loader/.test(dockerfile),
			"Docker must not run the minifier outside the leptoskit build",
		);
		const plugin = readFileSync(
			join(repoRoot, "scripts/leptoskit-plugin/web-assets-plugin.ts"),
			"utf8",
		);
		assert.ok(
			plugin.includes('from "../minify-split-loader.ts"'),
			"the plugin must reuse the guarded minifier implementation",
		);
	});

	test("refuses output that drops an exported split initializer", () => {
		const siteRoot = scenario("loader-site");
		const fixtureLoader = readFileSync(join(fixtureRoot, "loader-site/pkg", loaderName));

		const fakeBin = join(siteRoot, "fake-esbuild.mjs");
		// A broken minifier that drops the last export line.
		writeFileSync(
			fakeBin,
			[
				"#!/usr/bin/env node",
				'import { readFileSync, writeFileSync } from "node:fs";',
				"const args = process.argv.slice(2);",
				'const source = args.find((arg) => !arg.startsWith("--"));',
				'const out = args.find((arg) => arg.startsWith("--outfile=")).slice("--outfile=".length);',
				'const lines = readFileSync(source, "utf8").split("\\n");',
				'const kept = lines.filter((line) => line.startsWith("export const"));',
				"kept.pop();",
				'writeFileSync(out, lines.filter((line) => !line.startsWith("export const")).concat(kept).join("\\n"));',
			].join("\n"),
		);
		chmodSync(fakeBin, 0o755);

		const result = runMinify(siteRoot, { MONOCHANGE_ESBUILD_BIN: fakeBin });
		assert.equal(result.status, 1);
		assert.match(result.stderr, /exported split initializers/);
		assert.match(result.stderr, new RegExp(pricingExport));
		// The failed rewrite must not touch the site.
		assert.equal(readLoader(siteRoot), fixtureLoader.toString("utf8"));
	});

	test("fails when the site has no split loader", () => {
		const siteRoot = scenario("no-loader-site");

		const result = runMinify(siteRoot);
		assert.equal(result.status, 1);
		assert.match(result.stderr, /no __wasm_split\.\*\.js loader found/);
	});

	test("fails and leaves the file untouched when esbuild rejects the loader", () => {
		const siteRoot = scenario("broken-loader-site");
		const fixtureLoader = readFileSync(join(fixtureRoot, "broken-loader-site/pkg", loaderName));

		const result = runMinify(siteRoot);
		assert.equal(result.status, 1);
		assert.match(result.stderr, /esbuild failed/);
		assert.equal(readLoader(siteRoot), fixtureLoader.toString("utf8"));
	});

	test("fails when MONOCHANGE_ESBUILD_BIN does not exist", () => {
		const siteRoot = scenario("loader-site");

		const result = runMinify(siteRoot, {
			MONOCHANGE_ESBUILD_BIN: join(siteRoot, "missing-esbuild"),
		});
		assert.equal(result.status, 1);
		assert.match(result.stderr, /MONOCHANGE_ESBUILD_BIN does not exist/);
	});
});

describe("minify-split-loader entry glue", () => {
	const roots: string[] = [];

	afterEach(() => {
		for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
	});

	const glueFixture = join(fixtureRoot, "glue-site/pkg/monochange_app.js");
	const esbuildBin = join(repoRoot, "node_modules/.bin/esbuild");

	function glueScenario(): { root: string; pkg: string } {
		const root = mkdtempSync(join(tmpdir(), "minify-entry-glue-"));
		roots.push(root);
		const pkg = join(root, "pkg");
		mkdirSync(pkg, { recursive: true });
		cpSync(join(fixtureRoot, "glue-site/pkg"), pkg, { recursive: true });

		return { root, pkg };
	}

	test("minifies the entry glue and preserves its export surface", () => {
		const { pkg } = glueScenario();
		const before = readFileSync(join(pkg, "monochange_app.js"), "utf8");
		const sizes = minifyEntryGlue(pkg, "monochange_app", esbuildBin);
		const after = readFileSync(join(pkg, "monochange_app.js"), "utf8");

		assert.ok(sizes.rawAfter < sizes.rawBefore, "minified glue must shrink");
		// The hydration script consumes `default` (init), `initSync`, and every
		// #[wasm_bindgen] export such as `hydrate`.
		assert.ok(after.includes("hydrate"), "named export hydrate must survive");
		assert.match(after, /as default/);
		assert.match(after, /initSync/);
		// The split loader import must stay a module specifier, not a mangled
		// identifier or a resolved path.
		assert.match(after, /from\s*"\.\/__wasm_split\..+\.js"/);
		assert.notEqual(after, before);
	});

	test("refuses glue output that drops an exported name", () => {
		const { pkg } = glueScenario();
		const before = readFileSync(join(pkg, "monochange_app.js"), "utf8");
		const fakeBin = join(pkg, "fake-esbuild.mjs");
		writeFileSync(
			fakeBin,
			[
				"#!/usr/bin/env node",
				'import { readFileSync, writeFileSync } from "node:fs";',
				"const args = process.argv.slice(2);",
				'const source = args.find((arg) => !arg.startsWith("--"));',
				'const out = args.find((arg) => arg.startsWith("--outfile=")).slice("--outfile=".length);',
				'const code = readFileSync(source, "utf8");',
				// A broken minifier that silently drops the hydrate export.
				'writeFileSync(out, code.replace(/export function hydrate[^}]*}/, ""));',
			].join("\n"),
		);
		chmodSync(fakeBin, 0o755);

		assert.throws(
			() => minifyEntryGlue(pkg, "monochange_app", fakeBin),
			/changed the exported names \(missing: hydrate\)/,
		);
		assert.equal(readFileSync(join(pkg, "monochange_app.js"), "utf8"), before);
	});

	test("fails when the entry glue is missing", () => {
		const { root } = glueScenario();
		rmSync(join(root, "pkg/monochange_app.js"));

		assert.throws(
			() => minifyEntryGlue(join(root, "pkg"), "monochange_app", esbuildBin),
			/could not read the entry glue/,
		);
	});
});
