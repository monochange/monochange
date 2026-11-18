#!/usr/bin/env node
// Minifies the wasm-split loader JavaScript that the build emits unminified
// next to the hydrated client bundle, and the entry glue
// (`{output_name}.js`) beside it — everything cargo-leptos' `js-minify`
// used to compress. The wasm-bindgen glue minification covers the main
// `{output_name}.js` glue only, so the
// `__wasm_split.<hash>.js` module would keep its per-chunk route comments and
// full identifiers — and fail the committed website size gate, which compares
// that file by name at the 5% per-file gzip tolerance (scripts/website-size.ts).
//
// Both rewrites are guarded: a loader is replaced only when the minified
// output still exports the same `__wasm_split_load___*` initializers and still
// references the same `/pkg/*.js` import and `./*.wasm` chunk URLs; an entry
// glue only when it still exports the same names (its `default` init,
// `initSync`, every `#[wasm_bindgen]` export) and still imports the same
// module specifiers. A minifier regression or an unexpected shape therefore
// fails the build instead of reaching a deployment.
//
// The real build pipeline invokes this through the leptoskit tooling plugin
// (scripts/leptoskit-plugin/), which runs it inside `leptoskit build` before
// release precompression. This file also stays a standalone CLI for manual
// verification:
//
//   pnpm node scripts/minify-split-loader.ts [--site-root app/target/site]
//
// esbuild is resolved from `MONOCHANGE_ESBUILD_BIN`, then the workspace
// `node_modules/.bin/esbuild` dev dependency, then `PATH`, so the same script
// runs in CI, in devenv, and in the Docker release builder (which installs a
// pinned esbuild binary directly). Progress goes to stderr; stdout stays
// clean so the leptoskit plugin protocol can carry its JSON reply there.
import { spawnSync } from "node:child_process";
import {
	existsSync,
	mkdtempSync,
	readFileSync,
	realpathSync,
	readdirSync,
	rmSync,
	statSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";
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

/** One loader's gzip sizes before and after minification. */
export type LoaderSizes = {
	name: string;
	gzipBefore: number;
	gzipAfter: number;
	rawBefore: number;
	rawAfter: number;
};

/** What minifying every split loader in one pkg directory did. */
export type MinifySummary = { loaders: LoaderSizes[]; esbuildBin: string };

/** One entry glue's gzip sizes before and after minification. */
export type GlueSizes = {
	name: string;
	gzipBefore: number;
	gzipAfter: number;
	rawBefore: number;
	rawAfter: number;
};

class MinifyError extends Error {}

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

/** Resolves the esbuild binary: override env, workspace devDependency, then PATH. */
export function resolveEsbuild(): string {
	const override = process.env.MONOCHANGE_ESBUILD_BIN;

	if (override !== undefined) {
		if (!existsSync(override)) {
			throw new MinifyError(`MONOCHANGE_ESBUILD_BIN does not exist: ${override}`);
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
		throw new MinifyError(`could not run esbuild (${esbuildBin}): ${result.error.message}`);
	}

	if (result.status !== 0) {
		const detail = (result.stderr ?? result.stdout ?? `exit code ${result.status}`).slice(0, 4000);
		throw new MinifyError(`esbuild failed for ${sourcePath}:\n${detail}`);
	}
}

/** Rewrites one split loader in place with the guarded minifier contract. */
function minifyLoader(esbuildBin: string, pkgDir: string, name: string): LoaderSizes {
	const path = join(pkgDir, name);
	const before = readFileSync(path);
	const workDir = mkdtempSync(join(tmpdir(), "minify-split-loader-"));
	let after: Buffer;
	try {
		const outPath = join(workDir, "loader.js");
		runEsbuild(esbuildBin, path, outPath);
		after = readFileSync(outPath);

		const beforeReferences = references(before.toString("utf8"));
		const afterReferences = references(after.toString("utf8"));
		const exportMismatch = referencesMismatch(beforeReferences.exports, afterReferences.exports);

		if (exportMismatch !== undefined) {
			throw new MinifyError(
				`minified ${name} changed the exported split initializers (${exportMismatch}); leaving it untouched`,
			);
		}

		const urlMismatch = referencesMismatch(beforeReferences.urls, afterReferences.urls);

		if (urlMismatch !== undefined) {
			throw new MinifyError(
				`minified ${name} changed the referenced wasm/js URLs (${urlMismatch}); leaving it untouched`,
			);
		}

		writeFileSync(path, after);
	} finally {
		rmSync(workDir, { recursive: true, force: true });
	}
	const written = readFileSync(path);
	const gzipBefore = gzipSync(before).byteLength;
	const gzipAfter = gzipSync(written).byteLength;
	log(
		`${name} gzip ${gzipBefore} → ${gzipAfter} bytes (raw ${before.byteLength} → ${written.byteLength})`,
	);

	return {
		name,
		gzipBefore,
		gzipAfter,
		rawBefore: before.byteLength,
		rawAfter: written.byteLength,
	};
}

/**
 * Minifies every `__wasm_split.*.js` loader in `pkgDir` in place.
 *
 * The rewrite is guarded: a loader is replaced only when the minified output
 * still exports the same `__wasm_split_load___*` initializers and still
 * references the same `/pkg/*.js` import and `./*.wasm` chunk URLs. Throws a
 * `MinifyError` naming the problem instead of touching the file otherwise.
 */
export function minifySplitLoaders(pkgDir: string, esbuildBin?: string): MinifySummary {
	const resolved = esbuildBin ?? resolveEsbuild();
	let names: string[];
	try {
		names = readdirSync(pkgDir);
	} catch (error) {
		throw new MinifyError(
			`could not read ${pkgDir}: ${error instanceof Error ? error.message : String(error)}; build the site first`,
		);
	}
	const loaders = names
		.filter((name) => SPLIT_LOADER.test(name) && statSync(join(pkgDir, name)).isFile())
		.toSorted();

	if (loaders.length === 0) {
		throw new MinifyError(`no __wasm_split.*.js loader found in ${pkgDir}; build the site first`);
	}

	const sizes = loaders.map((name) => minifyLoader(resolved, pkgDir, name));
	const gzipBefore = sizes.reduce((total, size) => total + size.gzipBefore, 0);
	const gzipAfter = sizes.reduce((total, size) => total + size.gzipAfter, 0);
	log(`minified ${sizes.length} split loader(s); gzip ${gzipBefore} → ${gzipAfter} bytes`);

	return { loaders: sizes, esbuildBin: resolved };
}

/** Progress output goes to stderr; stdout carries only the plugin's JSON reply. */
function log(message: string): void {
	console.error(message);
}

// The entry glue's exported surface: `export function name(...)`,
// `export const/let/var name = …`, and every identifier in
// `export { a, b as c }` lists (the export is the name after `as`, or the
// bare identifier). Unanchored: the minified glue is one line, so exports
// appear mid-line.
const EXPORTED_FUNCTION = /\bexport\s+(?:async\s+)?function\s+(?<name>[A-Za-z0-9_$]+)/gu;
const EXPORTED_BINDING = /\bexport\s+(?:const|let|var)\s+(?<name>[A-Za-z0-9_$]+)/gu;
const EXPORT_LIST = /\bexport\s*\{(?<names>[^}]+)\}/gu;
// Static `import ... from "…"`, re-export `export … from "…"`, and dynamic
// `import("…")` module specifiers: the sibling modules the glue loads.
const MODULE_SPECIFIER = /(?:\bfrom\s*|\bimport\s*\(\s*)"(?<specifier>[^"]+)"/gu;

type GlueSurface = { exports: string[]; specifiers: string[] };

function glueSurface(code: string): GlueSurface {
	const names = new Set<string>();

	for (const match of code.matchAll(EXPORTED_FUNCTION)) {
		if (match.groups?.name !== undefined) names.add(match.groups.name);
	}

	for (const match of code.matchAll(EXPORTED_BINDING)) {
		if (match.groups?.name !== undefined) names.add(match.groups.name);
	}

	for (const list of code.matchAll(EXPORT_LIST)) {
		for (const entry of (list.groups?.names ?? "").split(",")) {
			// `local as exported` exports the name after `as`; a bare entry
			// exports itself.
			const exported =
				entry
					.trim()
					.split(/\s+as\s+/)
					.at(-1) ?? "";

			if (exported.length > 0) names.add(exported);
		}
	}

	const specifiers = new Set<string>();

	for (const match of code.matchAll(MODULE_SPECIFIER)) {
		if (match.groups?.specifier !== undefined) specifiers.add(match.groups.specifier);
	}

	return { exports: [...names].toSorted(), specifiers: [...specifiers].toSorted() };
}

/**
 * Rewrites the entry glue `<outputName>.js` in place with the guarded
 * minifier contract.
 *
 * The glue is replaced only when the minified output still exports the same
 * names (`default` init, `initSync`, every `#[wasm_bindgen]` export such as
 * `hydrate`) and still imports the same module specifiers (the wasm-split
 * loader and any sibling module). Throws a `MinifyError` naming the problem
 * instead of touching the file otherwise.
 */
export function minifyEntryGlue(
	pkgDir: string,
	outputName: string,
	esbuildBin?: string,
): GlueSizes {
	const resolved = esbuildBin ?? resolveEsbuild();
	const name = `${outputName}.js`;
	const path = join(pkgDir, name);
	let before: Buffer;

	try {
		before = readFileSync(path);
	} catch (error) {
		throw new MinifyError(
			`could not read the entry glue ${path}: ${error instanceof Error ? error.message : String(error)}; build the site first`,
		);
	}

	const workDir = mkdtempSync(join(tmpdir(), "minify-entry-glue-"));
	let after: Buffer;
	try {
		const outPath = join(workDir, "glue.js");
		runEsbuild(resolved, path, outPath);
		after = readFileSync(outPath);

		const beforeSurface = glueSurface(before.toString("utf8"));
		const afterSurface = glueSurface(after.toString("utf8"));
		const exportMismatch = referencesMismatch(beforeSurface.exports, afterSurface.exports);

		if (exportMismatch !== undefined) {
			throw new MinifyError(
				`minified ${name} changed the exported names (${exportMismatch}); leaving it untouched`,
			);
		}

		const specifierMismatch = referencesMismatch(beforeSurface.specifiers, afterSurface.specifiers);

		if (specifierMismatch !== undefined) {
			throw new MinifyError(
				`minified ${name} changed the imported module specifiers (${specifierMismatch}); leaving it untouched`,
			);
		}

		writeFileSync(path, after);
	} finally {
		rmSync(workDir, { recursive: true, force: true });
	}
	const written = readFileSync(path);
	const gzipBefore = gzipSync(before).byteLength;
	const gzipAfter = gzipSync(written).byteLength;
	log(
		`${name} gzip ${gzipBefore} → ${gzipAfter} bytes (raw ${before.byteLength} → ${written.byteLength})`,
	);

	return {
		name,
		gzipBefore,
		gzipAfter,
		rawBefore: before.byteLength,
		rawAfter: written.byteLength,
	};
}

function main() {
	const options = parseOptions(process.argv.slice(2), ["--site-root"]);
	const siteRoot = resolve(repoRoot, options.site_root ?? "app/target/site");
	const pkgDir = join(siteRoot, "pkg");

	try {
		minifySplitLoaders(pkgDir);
	} catch (error) {
		die(error instanceof Error ? error.message : String(error));
	}
}

// Run the CLI only when this file is the entry module. The entry path is
// canonicalized because the ESM loader resolves import.meta.url through
// real paths (macOS /var → /private/var), while argv[1] keeps the
// invocation path.
const entryPath = process.argv[1] ? pathToFileURL(realpathSync(process.argv[1])).href : undefined;

if (entryPath !== undefined && import.meta.url === entryPath) {
	main();
}
