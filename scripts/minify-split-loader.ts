#!/usr/bin/env node
// Minifies the wasm-split loader JavaScript that cargo-leptos emits unminified
// next to the hydrated client bundle. `js-minify = true` in
// [package.metadata.leptos] runs SWC over the main `{output_name}.js` glue
// only, so the `__wasm_split.*.js` module keeps its per-chunk route comments
// and full identifiers — and fails the committed website size gate, which
// compares that file by name at the 5% per-file gzip tolerance
// (scripts/website-size.ts). The upstream leptoskit web build
// (`leptoskit build --release`) emits the same loader shape, so this step
// applies to both paths.
//
// The rewrite is guarded: a loader is replaced only when the minified output
// still exports the same `__wasm_split_load___*` initializers and still
// references the same `/pkg/*.js` import and `./*.wasm` chunk URLs. A minifier
// regression or an unexpected loader shape therefore fails the build instead
// of reaching a deployment.
//
// Usage:
//   pnpm node scripts/minify-split-loader.ts [--site-root app/target/site]
//
// esbuild is resolved from `MONOCHANGE_ESBUILD_BIN`, then the workspace
// `node_modules/.bin/esbuild` dev dependency, then `PATH`, so the same script
// runs in CI, in devenv, and in the Docker release builder (which installs a
// pinned esbuild binary directly).
//
// This is part of the release build: CI runs it between
// `cargo leptos build --release --split` and the size/benchmark checks, and
// the Docker builder runs it before the site is copied out and precompressed.

import { spawnSync } from "node:child_process";
import {
	existsSync,
	mkdtempSync,
	readFileSync,
	readdirSync,
	rmSync,
	statSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..");
const ESBUILD_TARGET = "es2022";

// The loader is named `__wasm_split.<hash>.js` (with a literal placeholder
// until cargo-leptos `hash-files` renames it); the manifest is JSON and never
// matches this pattern.
const SPLIT_LOADER = /^__wasm_split\..+\.js$/u;

// Named exports the hydrated glue imports through
// `import * from "./__wasm_split.<hash>.js"`, one per lazy route.
const EXPORTED_INITIALIZER = /__wasm_split_load___[A-Za-z0-9_]+/gu;

// Strings the loader resolves at runtime: the `/pkg/{output_name}.js` shared
// imports module and every `./{chunk}.wasm` split chunk.
const LOADER_URL_STRING = /"((?:\.\/|\/pkg\/)[A-Za-z0-9_./-]+\.(?:wasm|js))"/gu;

type LoaderReferences = { exports: string[]; urls: string[] };

function die(message: string): never {
	console.error(message);
	process.exit(1);
}

function parseOptions(args: string[], names: string[]): Record<string, string> {
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

function uniqueSorted(matches: string[]): string[] {
	return [...new Set(matches)].toSorted();
}

function references(code: string): LoaderReferences {
	return {
		exports: uniqueSorted([...code.matchAll(EXPORTED_INITIALIZER)].map((match) => match[0])),
		urls: uniqueSorted([...code.matchAll(LOADER_URL_STRING)].map((match) => match[1])),
	};
}

function resolveEsbuild(): string {
	const override = process.env.MONOCHANGE_ESBUILD_BIN;
	if (override !== undefined) {
		if (!existsSync(override)) {
			die(`MONOCHANGE_ESBUILD_BIN does not exist: ${override}`);
		}
		return override;
	}
	const workspaceBin = join(repoRoot, "node_modules/.bin/esbuild");
	if (existsSync(workspaceBin)) return workspaceBin;
	return "esbuild";
}

function referencesMismatch(before: string[], after: string[]): string | undefined {
	const missing = before.filter((entry) => !after.includes(entry));
	if (missing.length > 0) return `missing: ${missing.join(", ")}`;
	const added = after.filter((entry) => !before.includes(entry));
	if (added.length > 0) return `unexpected additions: ${added.join(", ")}`;
	return undefined;
}

function runEsbuild(esbuildBin: string, sourcePath: string, outPath: string): void {
	const result = spawnSync(
		esbuildBin,
		[
			sourcePath,
			"--minify",
			"--format=esm",
			`--target=${ESBUILD_TARGET}`,
			"--log-level=warning",
			`--outfile=${outPath}`,
		],
		{ encoding: "utf8" },
	);
	if (result.error !== undefined) {
		die(`could not run esbuild (${esbuildBin}): ${result.error.message}`);
	}
	if (result.status !== 0) {
		const detail = (result.stderr ?? result.stdout ?? `exit code ${result.status}`).slice(0, 4000);
		die(`esbuild failed for ${sourcePath}:\n${detail}`);
	}
}

function minifyLoader(
	esbuildBin: string,
	pkgDir: string,
	name: string,
): { gzipBefore: number; gzipAfter: number } {
	const path = join(pkgDir, name);
	const before = readFileSync(path);
	const workDir = mkdtempSync(join(tmpdir(), "minify-split-loader-"));
	try {
		const outPath = join(workDir, "loader.js");
		runEsbuild(esbuildBin, path, outPath);
		const after = readFileSync(outPath);

		const beforeReferences = references(before.toString("utf8"));
		const afterReferences = references(after.toString("utf8"));
		const exportMismatch = referencesMismatch(beforeReferences.exports, afterReferences.exports);
		if (exportMismatch !== undefined) {
			die(
				`minified ${name} changed the exported split initializers (${exportMismatch}); leaving it untouched`,
			);
		}
		const urlMismatch = referencesMismatch(beforeReferences.urls, afterReferences.urls);
		if (urlMismatch !== undefined) {
			die(
				`minified ${name} changed the referenced wasm/js URLs (${urlMismatch}); leaving it untouched`,
			);
		}

		writeFileSync(path, after);
	} finally {
		rmSync(workDir, { recursive: true, force: true });
	}
	const after = readFileSync(path);
	const gzipBefore = gzipSync(before).byteLength;
	const gzipAfter = gzipSync(after).byteLength;
	console.log(
		`${name} gzip ${gzipBefore} → ${gzipAfter} bytes (raw ${before.byteLength} → ${after.byteLength})`,
	);
	return { gzipBefore, gzipAfter };
}

function main() {
	const options = parseOptions(process.argv.slice(2), ["--site-root"]);
	const siteRoot = resolve(repoRoot, options.site_root ?? "app/target/site");
	const pkgDir = join(siteRoot, "pkg");

	let names: string[];
	try {
		names = readdirSync(pkgDir);
	} catch (error) {
		die(
			`could not read ${pkgDir}: ${error instanceof Error ? error.message : String(error)}; build the site first with cargo leptos build --release --split`,
		);
	}
	const loaders = names
		.filter((name) => SPLIT_LOADER.test(name) && statSync(join(pkgDir, name)).isFile())
		.toSorted();
	if (loaders.length === 0) {
		die(
			`no __wasm_split.*.js loader found in ${pkgDir}; build the site first with cargo leptos build --release --split`,
		);
	}

	const esbuildBin = resolveEsbuild();
	let gzipBefore = 0;
	let gzipAfter = 0;
	for (const name of loaders) {
		const sizes = minifyLoader(esbuildBin, pkgDir, name);
		gzipBefore += sizes.gzipBefore;
		gzipAfter += sizes.gzipAfter;
	}
	console.log(
		`minified ${loaders.length} split loader(s); gzip ${gzipBefore} → ${gzipAfter} bytes`,
	);
}

main();
