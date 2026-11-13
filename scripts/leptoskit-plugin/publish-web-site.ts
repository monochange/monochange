#!/usr/bin/env node
// Publishes the assembled monochange web site after BOTH frontends built.
//
// The upstream leptoskit build compiles each frontend into its own scratch
// output directory and finishes by reporting and budget-checking that
// scratch bundle (crates/leptoskit_cli/src/build.rs at the pinned revision:
// `after_build` runs before the bundle report and the `bundle.budget_kib`
// check). The tooling plugin therefore only prepares each scratch bundle in
// place, and this publisher runs as a separate step after both
// `leptoskit build` commands succeeded — a build the SDK's own budget
// rejects never reaches the deployed tree, and its report measures real
// files.
//
// Layout (build:web in devenv.nix creates the run root under the ignored
// `.monochange/local/web-build/<run>/`; Docker and CI use the same shape):
//
//   <web-root>/web/           the website build's `--out` scratch output
//   <web-root>/projects-web/  the projects build's `--out` scratch output
//   <site-root>/              the published tree (default app/target/site)
//
// The published tree carries both namespaces (`pkg/site/`,
// `pkg/projects/`), the website build's public assets at the root, the
// deployed WOFF2 fonts, and ONE shared stylesheet at
// `pkg/site/monochange_app.css` (the stylesheet both frontends reference).
// Both scratch builds compile the same canonical Tailwind input, so their
// CSS must be byte-identical; exactly one copy ships. Every generation is
// staged from the current scratch outputs alone, so removed public files,
// renamed routes, and stale chunks cannot survive a rebuild.
//
// Publication contract
// --------------------
//
// Staging, compression, and validation all happen in a `.site-staging-*`
// directory created inside the site root's parent — the same directory, so
// promotion is same-filesystem rename. Promotion is a two-rename swap:
// `site` → `.site-previous-*`, then staging → `site`, then the previous
// tree is deleted. Each rename is atomic per directory entry, but the gap
// between the two renames is NOT atomic as a whole: a reader that opens the
// site root in that window sees a missing directory. Every in-repo consumer
// tolerates this by construction — the Docker image copies the tree only
// after `build:web` exits, and local servers start afterwards — and a
// failure of the second rename rolls the first back, so any failure leaves
// the last good published tree exactly as it was. This window is a
// documented property of the contract, not an atomicity claim.
//
// Logs go to stderr; the exit code carries the result.
import {
	cpSync,
	existsSync,
	mkdirSync,
	readFileSync,
	readdirSync,
	realpathSync,
	renameSync,
	rmSync,
	statSync,
	writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";
import { brotliCompressSync, constants as zlibConstants, gzipSync } from "node:zlib";

/** One frontend's assembly coordinates. */
export type FrontendSpec = {
	/** The scratch out directory name under the web root. */
	outDir: string;
	/** The published pkg namespace (also the URL prefix under /pkg). */
	namespace: string;
	/** The browser entry's file stem (the client lib crate name). */
	outputName: string;
};

/** The frontends every complete site carries, in assembly order. */
export const FRONTENDS: readonly FrontendSpec[] = [
	{ outDir: "web", namespace: "site", outputName: "monochange_app" },
	{
		outDir: "projects-web",
		namespace: "projects",
		outputName: "monochange_projects_web",
	},
];

/** The shared stylesheet's name inside each scratch pkg and the site pkg. */
export const SHARED_STYLESHEET = "monochange_app.css";

/** The WOFF2 subsets the plugin must leave in the website scratch output. */
export const FONT_NAMES = ["bricolage-800", "hanken-400", "hanken-700"];

/** Public assets the document shell references; a site without them is broken. */
export const REQUIRED_PUBLIC_ASSETS = [
	"favicon.ico",
	"favicon.svg",
	"site.webmanifest",
	"branding/mark.svg",
	"branding/social.png",
];

/** The wasm-split loader, `__wasm_split.<hash>.js`. */
const SPLIT_LOADER = /^__wasm_split\..+\.js$/u;
/** Chunk file stems the split manifest may reference. */
const CHUNK_STEM = /^(?:chunk_\d+|split___[a-z0-9_]+_\d+)$/u;
/** Chunk wasm files, referenced or stale. */
const CHUNK_FILE = /^(?:chunk_\d+|split___[a-z0-9_]+_\d+)\.wasm$/u;
// The compressible set upstream's compress.rs precompresses.
const COMPRESSIBLE = /\.(?:wasm|js|mjs|css|html|svg|json|map|txt|xml|webmanifest)$/u;
// Upstream's twin names: `<file>.gz` and `<file>.br` beside the source.
const PRECOMPRESSED = /\.(?:gz|br)$/u;

export class PublishFailure extends Error {}

function fail(message: string): never {
	throw new PublishFailure(message);
}

function log(message: string): void {
	console.error(`[publish-web-site] ${message}`);
}

/** The files a complete namespace carries besides its chunks. */
function requiredFiles(spec: FrontendSpec): string[] {
	return [`${spec.outputName}.js`, `${spec.outputName}_bg.wasm`, "__wasm_split_manifest.json"];
}

function readManifest(path: string, label: string): Set<string> {
	let manifest: unknown;
	try {
		manifest = JSON.parse(readFileSync(path, "utf8"));
	} catch (error) {
		fail(
			`could not read the ${label} split manifest ${path}: ${
				error instanceof Error ? error.message : String(error)
			}`,
		);
	}
	if (typeof manifest !== "object" || manifest === null || Array.isArray(manifest)) {
		fail(`invalid split manifest ${path}`);
	}
	const files = new Set<string>();
	for (const chunks of Object.values(manifest as Record<string, unknown>)) {
		if (
			!Array.isArray(chunks) ||
			chunks.some((chunk) => typeof chunk !== "string" || !CHUNK_STEM.test(chunk))
		) {
			fail(`invalid split manifest ${path}`);
		}
		for (const chunk of chunks as string[]) files.add(`${chunk}.wasm`);
	}
	if (files.size === 0) fail(`invalid split manifest ${path}: no split modules`);

	return files;
}

/**
 * Validates one scratch build output: present, complete, and internally
 * consistent — entry glue and wasm, split loader, a parseable manifest whose
 * chunks all exist and which references every chunk file present, and the
 * compiled shared stylesheet.
 *
 * # Errors
 *
 * Throws [`PublishFailure`] naming the first violated invariant.
 */
export function validateScratch(webRoot: string, spec: FrontendSpec): { chunks: Set<string> } {
	const outDir = join(webRoot, spec.outDir);
	if (!existsSync(outDir) || !statSync(outDir).isDirectory()) {
		fail(`no ${spec.namespace} scratch build at ${outDir}; both web builds must run first`);
	}
	const pkgDir = join(outDir, "pkg");
	if (!existsSync(pkgDir) || !statSync(pkgDir).isDirectory()) {
		fail(`the ${spec.namespace} scratch build at ${outDir} produced no pkg/ directory`);
	}
	for (const name of requiredFiles(spec)) {
		const path = join(pkgDir, name);
		if (!existsSync(path) || statSync(path).size === 0) {
			fail(`the ${spec.namespace} scratch build is missing pkg/${name}`);
		}
	}
	const loader = readdirSync(pkgDir).find((name) => SPLIT_LOADER.test(name));
	if (loader === undefined) {
		fail(`the ${spec.namespace} scratch build has no wasm-split loader in pkg/`);
	}
	const chunks = readManifest(join(pkgDir, "__wasm_split_manifest.json"), spec.namespace);
	for (const chunk of chunks) {
		if (!existsSync(join(pkgDir, chunk))) {
			fail(`the ${spec.namespace} split manifest references missing chunk pkg/${chunk}`);
		}
	}
	for (const name of readdirSync(pkgDir)) {
		if (CHUNK_FILE.test(name) && !chunks.has(name)) {
			fail(`the ${spec.namespace} scratch pkg carries unreferenced chunk pkg/${name}`);
		}
	}
	const css = join(pkgDir, SHARED_STYLESHEET);
	if (!existsSync(css) || statSync(css).size === 0) {
		fail(
			`the ${spec.namespace} scratch build has no ${SHARED_STYLESHEET}; the plugin must compile it`,
		);
	}

	return { chunks };
}

/** The files under `root`, relative to it, sorted. */
export function listTree(root: string): string[] {
	const collected: string[] = [];
	const walk = (directory: string, prefix: string) => {
		for (const entry of readdirSync(directory, { withFileTypes: true })) {
			const path = join(directory, entry.name);
			const name = prefix.length > 0 ? `${prefix}/${entry.name}` : entry.name;
			if (entry.isDirectory()) walk(path, name);
			else if (entry.isFile()) collected.push(name);
		}
	};
	walk(root, "");

	return collected.toSorted();
}

/** Precompresses every compressible file in `root` with upstream's twins. */
export function precompressTree(root: string): { gzip: number; brotli: number } {
	let gzipCount = 0;
	let brotliCount = 0;
	const stack = [root];
	while (stack.length > 0) {
		const directory = stack.pop();
		if (directory === undefined) fail("unreachable directory entry");
		for (const entry of readdirSync(directory, { withFileTypes: true })) {
			const path = join(directory, entry.name);
			if (entry.isDirectory()) {
				stack.push(path);
				continue;
			}
			if (!entry.isFile() || PRECOMPRESSED.test(path) || !COMPRESSIBLE.test(path)) continue;
			const raw = readFileSync(path);
			// The staging tree is fresh, so twins are new files here; writeFileSync
			// replaces any leftover from an interrupted run of the same process.
			writeFileSync(`${path}.gz`, gzipSync(raw, { level: 9 }));
			writeFileSync(
				`${path}.br`,
				brotliCompressSync(raw, {
					params: {
						[zlibConstants.BROTLI_PARAM_QUALITY]: 11,
						[zlibConstants.BROTLI_PARAM_LGWIN]: 24,
					},
				}),
			);
			gzipCount += 1;
			brotliCount += 1;
		}
	}

	return { gzip: gzipCount, brotli: brotliCount };
}

/**
 * Validates a staged generation before promotion: both namespaces complete
 * with consistent manifests, exactly one shared stylesheet, the deployed
 * fonts and shell-referenced public assets present, no per-namespace CSS in
 * the projects namespace, and every compressible asset carrying nonempty
 * gzip and brotli twins. A partial generation is never promoted.
 *
 * # Errors
 *
 * Throws [`PublishFailure`] naming the first violated invariant.
 */
export function validateGeneration(staging: string): void {
	for (const spec of FRONTENDS) {
		const namespaceDir = join(staging, "pkg", spec.namespace);
		if (!existsSync(namespaceDir)) {
			fail(`the staged generation is missing pkg/${spec.namespace}/`);
		}
		for (const name of requiredFiles(spec)) {
			const path = join(namespaceDir, name);
			if (!existsSync(path) || statSync(path).size === 0) {
				fail(`the staged generation is missing pkg/${spec.namespace}/${name}`);
			}
		}
		if (!readdirSync(namespaceDir).some((name) => SPLIT_LOADER.test(name))) {
			fail(`the staged generation has no wasm-split loader in pkg/${spec.namespace}/`);
		}
		const chunks = readManifest(join(namespaceDir, "__wasm_split_manifest.json"), spec.namespace);
		for (const chunk of chunks) {
			if (!existsSync(join(namespaceDir, chunk))) {
				fail(
					`the staged generation's ${spec.namespace} manifest references missing pkg/${spec.namespace}/${chunk}`,
				);
			}
		}
	}
	const sharedCss = join(staging, "pkg", "site", SHARED_STYLESHEET);
	if (!existsSync(sharedCss) || statSync(sharedCss).size === 0) {
		fail(`the staged generation is missing the shared stylesheet pkg/site/${SHARED_STYLESHEET}`);
	}
	const projectsCss = join(staging, "pkg", "projects", SHARED_STYLESHEET);
	if (existsSync(projectsCss)) {
		fail(
			`the staged generation ships a duplicate stylesheet pkg/projects/${SHARED_STYLESHEET}; ` +
				`only pkg/site/${SHARED_STYLESHEET} may ship`,
		);
	}
	for (const font of FONT_NAMES) {
		const path = join(staging, "fonts", `${font}.woff2`);
		if (!existsSync(path) || statSync(path).size === 0) {
			fail(`the staged generation is missing the deployed font fonts/${font}.woff2`);
		}
	}
	for (const asset of REQUIRED_PUBLIC_ASSETS) {
		if (!existsSync(join(staging, asset))) {
			fail(`the staged generation is missing the public asset ${asset}`);
		}
	}
	for (const name of listTree(staging)) {
		if (PRECOMPRESSED.test(name) || !COMPRESSIBLE.test(name)) continue;
		for (const twin of [`${name}.gz`, `${name}.br`]) {
			const path = join(staging, twin);
			if (!existsSync(path) || statSync(path).size === 0) {
				fail(
					`the staged generation has no ${twin.endsWith(".gz") ? "gzip" : "brotli"} twin for ${name}`,
				);
			}
		}
	}
}

/**
 * Promotes a validated staging directory to `site` with the documented
 * two-rename swap. When the second rename fails, the first is rolled back,
 * so `site` holds exactly what it held before on every failure path.
 *
 * # Errors
 *
 * Throws [`PublishFailure`] when the swap fails.
 */
export function promoteGeneration(staging: string, site: string): void {
	if (!existsSync(staging) || !statSync(staging).isDirectory()) {
		fail(`the staged generation ${staging} vanished before promotion`);
	}
	const tag = `${Date.now()}-${process.pid}`;
	const previous = join(dirname(site), `.site-previous-${tag}`);
	const hadSite = existsSync(site);
	if (hadSite) {
		renameSync(site, previous);
	}
	try {
		renameSync(staging, site);
	} catch (error) {
		if (hadSite) {
			renameSync(previous, site);
		}
		fail(`could not promote the staged generation: ${String(error)}`);
	}
	// The new site is complete: retire the superseded tree.
	if (hadSite) {
		rmSync(previous, { recursive: true, force: true });
	}
}

/** A unique staging directory inside the site root's parent (same filesystem). */
function stagingPath(site: string): string {
	const parent = dirname(site);
	mkdirSync(parent, { recursive: true });
	// Remove leftovers from a crashed publication first: they are owned
	// artifacts and never part of a live site.
	for (const entry of readdirSync(parent)) {
		if (entry.startsWith(".site-staging-") || entry.startsWith(".site-previous-")) {
			rmSync(join(parent, entry), { recursive: true, force: true });
		}
	}

	return join(parent, `.site-staging-${Date.now()}-${process.pid}`);
}

/** Copies a directory tree, skipping `skip` names at the tree's root. */
function copyTree(source: string, target: string, skip: (name: string) => boolean): void {
	mkdirSync(target, { recursive: true });
	for (const entry of readdirSync(source, { withFileTypes: true })) {
		if (skip(entry.name)) continue;
		const from = join(source, entry.name);
		const to = join(target, entry.name);
		if (entry.isDirectory()) {
			copyTree(from, to, () => false);
		} else if (entry.isFile()) {
			cpSync(from, to);
		}
	}
}

/**
 * Assembles, validates, and publishes the site from both scratch builds.
 *
 * # Errors
 *
 * Throws [`PublishFailure`] on any validation, compression, or promotion
 * failure; the previously published site is left untouched and no partial
 * staging directory is left behind.
 */
export function publishSite(
	webRoot: string,
	siteRoot: string,
): {
	files: number;
	gzip: number;
	brotli: number;
	site: string;
} {
	const site = resolve(siteRoot);

	// 1. Validate the fresh inputs before staging anything.
	const cssBytes = new Map<string, Buffer>();
	for (const spec of FRONTENDS) {
		validateScratch(webRoot, spec);
		cssBytes.set(
			spec.namespace,
			readFileSync(join(webRoot, spec.outDir, "pkg", SHARED_STYLESHEET)),
		);
	}

	// 2. One stylesheet: both entries compile the same canonical Tailwind
	// input, so their bytes must agree. A divergence means the compile is
	// not source-independent and must not ship silently.
	const [websiteCss, projectsCss] = [...cssBytes.values()];
	if (!websiteCss.equals(projectsCss)) {
		fail(
			"the two scratch builds compiled different stylesheets; the shared Tailwind input " +
				"must produce identical CSS for both frontends",
		);
	}

	// 3. Stage a complete replacement from the current scratch outputs alone,
	// inside the site root's parent so promotion renames stay on one
	// filesystem (a web root on another device — a Docker cache mount —
	// cannot break the swap).
	const staging = stagingPath(site);
	try {
		for (const spec of FRONTENDS) {
			copyTree(
				join(webRoot, spec.outDir, "pkg"),
				join(staging, "pkg", spec.namespace),
				// The projects namespace must not ship a duplicate stylesheet;
				// the site namespace's copy is the one both frontends reference.
				(name) => spec.namespace !== "site" && name === SHARED_STYLESHEET,
			);
		}
		// The website build contributes the shared public assets at the root;
		// its WOFF2 subsets become the deployed fonts, replacing the TTF
		// sources the SDK's public mirror carries.
		copyTree(
			join(webRoot, FRONTENDS[0].outDir),
			staging,
			(name) => name === "pkg" || name === ".font-subsets",
		);
		const fonts = join(staging, "fonts");
		mkdirSync(fonts, { recursive: true });
		for (const font of FONT_NAMES) {
			rmSync(join(fonts, `${font}.ttf`), { force: true });
			cpSync(
				join(webRoot, FRONTENDS[0].outDir, ".font-subsets", `${font}.woff2`),
				join(fonts, `${font}.woff2`),
			);
		}

		// 4. Compress the complete tree once, exactly as the served bytes.
		const compressed = precompressTree(staging);

		// 5. Validate the staged generation before anyone can see it.
		validateGeneration(staging);

		// 6. Promote: last-good survives every failure up to and during the swap.
		promoteGeneration(staging, site);
		const files = listTree(site).length;
		log(
			`published ${site}: ${files} files (${compressed.gzip} gzip, ${compressed.brotli} brotli twins)`,
		);

		return { files, gzip: compressed.gzip, brotli: compressed.brotli, site };
	} catch (error) {
		// Never leave a partial staging tree behind: the published site is
		// untouched and the scratch inputs stay for the SDK's own reporting.
		rmSync(staging, { recursive: true, force: true });
		throw error;
	}
}

function main(): void {
	const args = process.argv.slice(2);
	const parse = (name: string): string => {
		const index = args.indexOf(name);
		if (index === -1 || args[index + 1] === undefined) {
			console.error(`usage: publish-web-site.ts --web-root <dir> --site-root <dir>`);
			process.exit(2);
		}

		return args[index + 1] as string;
	};
	if (args.length !== 4 || !args.includes("--web-root") || !args.includes("--site-root")) {
		console.error("usage: publish-web-site.ts --web-root <dir> --site-root <dir>");
		process.exit(2);
	}
	const webRoot = resolve(parse("--web-root"));
	const siteRoot = parse("--site-root");

	try {
		publishSite(webRoot, siteRoot);
	} catch (error) {
		log(error instanceof Error ? error.message : String(error));
		process.exit(1);
	}
}

// Run the CLI only when this file is the entry module, so tests can import
// the pure helpers. The entry path is canonicalized because the ESM loader
// resolves import.meta.url through real paths (macOS /var → /private/var),
// while argv[1] keeps the invocation path.
const entryPath = process.argv[1] ? pathToFileURL(realpathSync(process.argv[1])).href : undefined;
if (entryPath !== undefined && import.meta.url === entryPath) {
	main();
}
