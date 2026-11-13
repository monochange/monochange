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
// Exclusive ownership. One publisher at a time owns a destination, through
// its lease: the directory `.<site>.publish-lease` beside the site root
// (`app/target/.site.publish-lease` for the default `app/target/site`). A
// publisher writes its owner record, `owner-<token>.json` (the random token
// in its name; pid, host, and start time inside, for the operator), into a
// claim directory only it names, then renames the claim to the lease. That
// rename fails while any lease exists (a lease always holds its owner
// record, so it is never an empty directory a rename could replace), so a
// lease is never shared and never exists without its owner record. A held
// lease fails every other publication before it changes anything.
//
// All transient state lives inside the lease: the generation is staged,
// compressed, and validated in `<lease>/staging`, and the superseded tree
// waits in `<lease>/previous`. Because both sit in the site root's parent,
// promotion is a same-filesystem rename. Nothing beside the site root is
// ever removed by name pattern, so other destinations sharing the parent,
// other owners' state, and unrelated files such as SQLite databases are
// never touched.
//
// Promotion is a two-rename swap: `site` → `<lease>/previous`, then
// `<lease>/staging` → `site`. Each rename is atomic per directory entry, but
// the gap between them is NOT atomic as a whole: a reader that opens the
// site root in that window sees a missing directory. Every in-repo consumer
// reads the tree only after the publisher exits — the Docker image copies it
// into the image before anything serves it, and local servers start
// afterwards — so no hot-deployment reader exists. This window is a
// documented property, not an atomicity claim.
//
// Outcomes:
//
// - Any failure before the swap leaves the site root untouched; releasing
//   the lease discards the staging tree.
// - A failed second rename rolls the first back. If the rollback fails too,
//   the last-good tree stays in `<lease>/previous` and the lease stays held,
//   so nothing can touch it; the error names both and the recovery command.
// - Once the second rename succeeds the new generation is published.
//   Releasing the lease renames it to `.<site>.publish-retired-<token>`,
//   which frees the destination in one step, and removes that directory with
//   the superseded tree. A failure there is a reported leftover, never a
//   failed publication; no later publication touches a retired directory.
// - The publisher never guesses that an owner is dead (a pid means nothing
//   across containers) and never breaks, steals, or cleans another owner's
//   lease. After an interrupted publication the operator confirms the owner
//   is gone, then runs `--site-root <dir> --recover-abandoned-lease <token>`.
//   Recovery adopts the lease by renaming its owner record, so concurrent
//   recoveries cannot both win. It restores `<lease>/previous` when the site
//   root is absent; otherwise it moves that tree aside to
//   `.<site>.publish-retained-<token>`, because it never deletes a tree it
//   cannot attribute. It then releases the lease, discarding only the
//   staging tree, which is never a published generation.
//
// Logs go to stderr. Exit codes: 0 published or recovered (cleanup leftovers
// are warnings), 1 failed, 2 usage.
import { randomBytes } from "node:crypto";
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
import { hostname } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
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

/**
 * A failure after which the lease must stay exactly as it is: it retains a
 * last-good tree, or it is no longer this publication's to release. Only an
 * explicit recovery may change it.
 */
export class RecoveryRequired extends PublishFailure {}

function fail(message: string): never {
	throw new PublishFailure(message);
}

function log(message: string): void {
	console.error(`[publish-web-site] ${message}`);
}

function errorText(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

/** The lease's staging tree: the generation being assembled. */
const STAGING = "staging";
/** The lease's superseded tree, between the swap and the lease's release. */
const PREVIOUS = "previous";
/** An owner token: 16 lowercase hex digits. */
const OWNER_TOKEN = /^[0-9a-f]{16}$/u;
/** An owner record's file name inside a lease. */
const OWNER_RECORD = /^owner-([0-9a-f]{16})\.json$/u;

/** A destination lease this process holds. */
type HeldLease = {
	/** The site root the lease guards. */
	readonly site: string;
	/** The lease directory, `.<site>.publish-lease` beside the site root. */
	readonly path: string;
	/** This owner's token; its record is `<path>/owner-<token>.json`. */
	readonly token: string;
};

/** What releasing a lease left behind. */
export type LeaseCleanup =
	| { readonly kind: "released" }
	/** Released, but the retired directory could not be removed and is retained. */
	| { readonly kind: "retained"; readonly path: string; readonly reason: string }
	/** The lease could not be released and stays held. */
	| { readonly kind: "held"; readonly path: string; readonly reason: string }
	/** The lease no longer carries this owner's record; it was left untouched. */
	| { readonly kind: "lost"; readonly path: string };

/** A publication artifact beside the site root: `.<site>.publish-<kind>`. */
function artifactPath(site: string, kind: string): string {
	return join(dirname(site), `.${basename(site)}.publish-${kind}`);
}

/** The destination's fixed lease directory. */
function leasePath(site: string): string {
	return artifactPath(site, "lease");
}

function ownerRecordName(token: string): string {
	return `owner-${token}.json`;
}

function newToken(): string {
	return randomBytes(8).toString("hex");
}

/** An owner record's contents; its file name carries the authoritative token. */
function ownerRecord(): string {
	const record = {
		pid: process.pid,
		host: hostname(),
		startedAt: new Date().toISOString(),
	};

	return `${JSON.stringify(record)}\n`;
}

/** The tokens of the owner records a lease holds; a lease this publisher wrote holds one. */
function ownerTokens(lease: string): string[] {
	let names: string[];
	try {
		names = readdirSync(lease);
	} catch {
		return [];
	}

	const tokens: string[] = [];
	for (const name of names) {
		const match = OWNER_RECORD.exec(name);
		if (match !== null) tokens.push(match[1]);
	}

	return tokens;
}

/** An owner's identity for diagnostics. The token is authoritative; the rest is informational. */
function describeOwner(lease: string, token: string): string {
	try {
		const record: unknown = JSON.parse(readFileSync(join(lease, ownerRecordName(token)), "utf8"));
		if (typeof record === "object" && record !== null && "pid" in record && "host" in record) {
			const started = "startedAt" in record ? `, started ${String(record.startedAt)}` : "";

			return `owner ${token} (pid ${String(record.pid)} on host ${String(record.host)}${started})`;
		}
	} catch {
		// An unreadable record still names its owner through its file name.
	}

	return `owner ${token}`;
}

function recoveryCommand(site: string, token: string): string {
	return (
		`node scripts/leptoskit-plugin/publish-web-site.ts --site-root ${JSON.stringify(site)} ` +
		`--recover-abandoned-lease ${token}`
	);
}

/** What a lease retains, read from the filesystem now so no claim outlives the tree. */
function retainedTrees(lease: string, site: string): string[] {
	const notes: string[] = [];
	const previous = join(lease, PREVIOUS);
	if (existsSync(previous)) {
		notes.push(
			existsSync(site)
				? `the tree ${previous}, retained while the site root also exists (recovery moves it aside, never deletes it)`
				: `the last-good tree ${previous}, while the site root is absent (recovery restores it)`,
		);
	}

	const staging = join(lease, STAGING);
	if (existsSync(staging)) {
		notes.push(`an uncommitted generation ${staging} (recovery discards it)`);
	}

	return notes;
}

/** Why a publication may not proceed while another lease holds the destination. */
function heldLeaseMessage(site: string): string {
	const lease = leasePath(site);
	const tokens = ownerTokens(lease);
	if (tokens.length !== 1) {
		return (
			`${site} has a publication lease ${lease} without exactly one owner record this ` +
			`publisher writes, so its owner cannot be identified; this publication changed nothing. ` +
			`Only after confirming no publisher is running, inspect ${lease} and remove it by hand.`
		);
	}

	const [token] = tokens;
	const retained = retainedTrees(lease, site);

	return [
		`${site} is owned by another publication: its lease ${lease} is held by ` +
			`${describeOwner(lease, token)}. This publication changed nothing.`,
		...(retained.length > 0 ? [`The lease retains ${retained.join("; and ")}.`] : []),
		"If that publisher is still running, wait for it to finish. Only after confirming it is " +
			"not running anywhere, recover the destination with:",
		`  ${recoveryCommand(site, token)}`,
	].join("\n");
}

/**
 * Takes exclusive ownership of `site`. The owner record is complete before
 * the claim is renamed to the lease, and that rename fails while any lease
 * exists.
 *
 * # Errors
 *
 * Throws [`PublishFailure`] describing the holder when the destination is
 * already leased; nothing is changed.
 */
function acquireLease(site: string): HeldLease {
	const token = newToken();
	const lease = leasePath(site);
	const claim = artifactPath(site, `claim-${token}`);

	mkdirSync(dirname(site), { recursive: true });
	mkdirSync(claim);
	writeFileSync(join(claim, ownerRecordName(token)), ownerRecord());
	try {
		renameSync(claim, lease);
	} catch (error) {
		rmSync(claim, { recursive: true, force: true });
		if (existsSync(lease)) fail(heldLeaseMessage(site));
		fail(`could not take the publication lease ${lease}: ${errorText(error)}`);
	}

	return { site, path: lease, token };
}

/**
 * Adopts the lease of an owner the operator confirmed is gone. Renaming the
 * owner's record is the compare-and-swap: it succeeds for exactly one
 * recovery, and only while that owner's record is still in the lease.
 *
 * # Errors
 *
 * Throws [`PublishFailure`] when no lease exists or `token` does not own it;
 * nothing is changed.
 */
function adoptLease(site: string, token: string): HeldLease {
	const lease = leasePath(site);
	const adopted = newToken();
	try {
		renameSync(join(lease, ownerRecordName(token)), join(lease, ownerRecordName(adopted)));
	} catch (error) {
		if (!existsSync(lease)) {
			fail(`no publication lease at ${lease}; nothing to recover and nothing was changed`);
		}

		const owners = ownerTokens(lease);
		fail(
			`the lease ${lease} is not held by owner ${token} (${errorText(error)}); its owner ` +
				`records name ${owners.length > 0 ? owners.join(", ") : "nobody"}. Nothing was changed.`,
		);
	}
	writeFileSync(join(lease, ownerRecordName(adopted)), ownerRecord());

	return { site, path: lease, token: adopted };
}

function owns(lease: HeldLease): boolean {
	return existsSync(join(lease.path, ownerRecordName(lease.token)));
}

/**
 * Ends this owner's lease: renames it to `.<site>.publish-retired-<token>`,
 * which frees the destination in one step, then removes that directory with
 * whatever it still holds — the superseded tree after a publication, the
 * discarded staging tree after a failure. Never throws.
 */
function releaseLease(lease: HeldLease): LeaseCleanup {
	if (!owns(lease)) return { kind: "lost", path: lease.path };

	const retired = artifactPath(lease.site, `retired-${lease.token}`);
	try {
		renameSync(lease.path, retired);
	} catch (error) {
		return { kind: "held", path: lease.path, reason: errorText(error) };
	}

	try {
		rmSync(retired, { recursive: true, force: true });
	} catch (error) {
		return { kind: "retained", path: retired, reason: errorText(error) };
	}

	return { kind: "released" };
}

function reportCleanup(cleanup: LeaseCleanup, lease: HeldLease): void {
	switch (cleanup.kind) {
		case "released":
			return;
		case "retained":
			log(
				`could not remove the retired lease ${cleanup.path} (${cleanup.reason}); it is ` +
					`retained and no publication touches it — remove it by hand`,
			);
			return;
		case "held": {
			const retained = retainedTrees(cleanup.path, lease.site);
			log(
				[
					`could not release the lease ${cleanup.path} (${cleanup.reason}); it stays held, so ` +
						`later publications fail closed.`,
					...(retained.length > 0 ? [`It retains ${retained.join("; and ")}.`] : []),
					"Once this process has exited, recover the destination with:",
					`  ${recoveryCommand(lease.site, lease.token)}`,
				].join("\n"),
			);
			return;
		}
		case "lost":
			log(
				`the lease ${cleanup.path} no longer carries this publication's owner record ` +
					`(owner ${lease.token}); it was left untouched`,
			);
			return;
		default: {
			const exhaustive: never = cleanup;

			return exhaustive;
		}
	}
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
 * Promotes `<lease>/staging` to the site root with the documented two-rename
 * swap. A failed second rename rolls the first back.
 *
 * # Errors
 *
 * Throws [`PublishFailure`] when the swap fails and the site root holds what
 * it held before, or [`RecoveryRequired`] when the rollback failed too (the
 * last-good tree then waits in the lease) or the lease is no longer owned.
 */
function promoteGeneration(lease: HeldLease): void {
	const { site } = lease;
	const staging = join(lease.path, STAGING);
	const previous = join(lease.path, PREVIOUS);

	// The swap is the first change to the site root: prove ownership first.
	if (!owns(lease)) {
		throw new RecoveryRequired(
			`the lease ${lease.path} no longer carries this publication's owner record (owner ` +
				`${lease.token}); the site root and the lease were left untouched`,
		);
	}

	const hadSite = existsSync(site);
	if (hadSite) {
		try {
			renameSync(site, previous);
		} catch (error) {
			fail(`could not move the published tree aside: ${errorText(error)}; ${site} is untouched`);
		}
	}

	try {
		renameSync(staging, site);
	} catch (error) {
		if (!hadSite) fail(`could not promote the staged generation: ${errorText(error)}`);

		try {
			renameSync(previous, site);
		} catch (rollbackError) {
			const failure =
				`could not promote the staged generation (${errorText(error)}) and could not ` +
				`restore the last-good tree (${errorText(rollbackError)}).`;
			if (!existsSync(previous)) {
				throw new RecoveryRequired(
					`${failure} The last-good tree is no longer at ${previous}; the lease ${lease.path} ` +
						`stays held — inspect it and ${site} by hand before removing anything.`,
				);
			}

			throw new RecoveryRequired(
				[
					`${failure} The last-good tree is retained at ${previous}; the lease ${lease.path} ` +
						`stays held by owner ${lease.token}, so no publication touches it.`,
					"Once this process has exited, recover the destination with:",
					`  ${recoveryCommand(site, lease.token)}`,
				].join("\n"),
			);
		}

		fail(
			`could not promote the staged generation: ${errorText(error)}; the last-good tree was ` +
				`restored to ${site}`,
		);
	}
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

/** A staged generation's file and twin counts. */
type StagedGeneration = { files: number; gzip: number; brotli: number };

/**
 * Stages a complete replacement from the current scratch outputs alone,
 * compresses it once exactly as the served bytes, and validates it.
 *
 * # Errors
 *
 * Throws [`PublishFailure`] when the staged generation is incomplete.
 */
function stageGeneration(webRoot: string, staging: string): StagedGeneration {
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

	const compressed = precompressTree(staging);
	validateGeneration(staging);

	return { files: listTree(staging).length, ...compressed };
}

/** A completed publication. */
export type PublishResult = StagedGeneration & {
	site: string;
	/** What releasing the lease left behind; a leftover never fails the publication. */
	cleanup: LeaseCleanup;
};

/**
 * Assembles, validates, and publishes the site from both scratch builds,
 * owning the destination's lease from staging through retirement.
 *
 * # Errors
 *
 * Throws [`PublishFailure`] on invalid inputs (nothing is touched), on a
 * held lease (nothing is changed), and on any staging, validation,
 * compression, or promotion failure (the site root keeps its last good
 * tree and the staging tree is discarded). Throws [`RecoveryRequired`] when
 * the swap and its rollback both failed: the last-good tree then waits in
 * the still-held lease, and the message names it and the recovery command.
 */
export function publishSite(webRoot: string, siteRoot: string): PublishResult {
	const site = resolve(siteRoot);

	// 1. Validate the fresh inputs before touching anything.
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

	// 3. Own the destination. The staging tree lives inside the lease, beside
	// the site root, so promotion renames stay on one filesystem (a web root
	// on another device — a Docker cache mount — cannot break the swap).
	const lease = acquireLease(site);

	// 4. Stage, compress, validate, and promote. The last-good tree survives
	// every failure up to and during the swap.
	let generation: StagedGeneration;
	try {
		generation = stageGeneration(webRoot, join(lease.path, STAGING));
		promoteGeneration(lease);
	} catch (error) {
		// A lease that retains the last-good tree, or is no longer ours, stays
		// as it is for explicit recovery. Otherwise the site root is untouched
		// and releasing the lease only discards the staging tree.
		if (!(error instanceof RecoveryRequired)) reportCleanup(releaseLease(lease), lease);
		throw error;
	}

	// 5. Published. Releasing the lease retires the superseded tree; a
	// leftover is reported, never a failed publication.
	const cleanup = releaseLease(lease);
	log(
		`published ${site}: ${generation.files} files (${generation.gzip} gzip, ` +
			`${generation.brotli} brotli twins)`,
	);
	reportCleanup(cleanup, lease);

	return { ...generation, site, cleanup };
}

/** What recovery did with the tree an abandoned lease retained. */
export type RecoveredTree =
	/** The lease held no tree. */
	| { readonly kind: "none" }
	/** The last-good tree was moved back to the absent site root. */
	| { readonly kind: "restored" }
	/** The site root exists, so the lease's tree was moved aside, never deleted. */
	| { readonly kind: "set-aside"; readonly path: string };

/** A completed recovery. */
export type RecoveryResult = {
	site: string;
	tree: RecoveredTree;
	cleanup: LeaseCleanup;
};

/**
 * Recovers a destination whose lease an interrupted publication abandoned.
 * Run it only after confirming that owner is not running anywhere: the
 * publisher never infers that on its own. Recovery adopts the lease, then
 * restores the retained last-good tree when the site root is absent, or
 * moves the retained tree aside to `.<site>.publish-retained-<token>` when
 * the site root exists, and finally releases the lease, discarding only an
 * uncommitted staging tree.
 *
 * # Errors
 *
 * Throws [`PublishFailure`] when `token` is malformed or does not own the
 * lease (nothing is changed), or [`RecoveryRequired`] when the retained
 * tree cannot be moved (the lease stays held by the adopting owner).
 */
export function recoverAbandonedLease(siteRoot: string, token: string): RecoveryResult {
	const site = resolve(siteRoot);
	if (!OWNER_TOKEN.test(token)) {
		fail(
			`invalid owner token ${JSON.stringify(token)}: expected the 16 hex digits a held-lease ` +
				`error names`,
		);
	}

	const lease = adoptLease(site, token);
	const previous = join(lease.path, PREVIOUS);
	let tree: RecoveredTree = { kind: "none" };
	if (existsSync(previous)) {
		const target = existsSync(site) ? artifactPath(site, `retained-${lease.token}`) : site;
		try {
			renameSync(previous, target);
		} catch (error) {
			throw new RecoveryRequired(
				[
					`could not move ${previous} to ${target}: ${errorText(error)}. The lease ` +
						`${lease.path} stays held, now by owner ${lease.token}; retry with:`,
					`  ${recoveryCommand(site, lease.token)}`,
				].join("\n"),
			);
		}
		tree = target === site ? { kind: "restored" } : { kind: "set-aside", path: target };
	}

	const cleanup = releaseLease(lease);
	switch (tree.kind) {
		case "none":
			log(`recovered ${site}: the lease retained no tree; the site root was left as found`);
			break;
		case "restored":
			log(`recovered ${site}: restored the last-good tree the lease retained`);
			break;
		case "set-aside":
			log(
				`recovered ${site}: the site root exists, so the lease's tree was moved to ` +
					`${tree.path} — inspect it, then remove it by hand`,
			);
			break;
		default: {
			const exhaustive: never = tree;

			return exhaustive;
		}
	}
	reportCleanup(cleanup, lease);

	return { site, tree, cleanup };
}

const USAGE = [
	"usage: publish-web-site.ts --web-root <dir> --site-root <dir>",
	"       publish-web-site.ts --site-root <dir> --recover-abandoned-lease <owner-token>",
].join("\n");

type Command =
	| { kind: "publish"; webRoot: string; siteRoot: string }
	| { kind: "recover"; siteRoot: string; token: string };

function parseCommand(args: string[]): Command | undefined {
	let values: { "web-root"?: string; "site-root"?: string; "recover-abandoned-lease"?: string };
	try {
		({ values } = parseArgs({
			args,
			options: {
				"web-root": { type: "string" },
				"site-root": { type: "string" },
				"recover-abandoned-lease": { type: "string" },
			},
		}));
	} catch {
		// parseArgs rejects unknown options, positionals, and missing values.
		return undefined;
	}

	const { "web-root": webRoot, "site-root": siteRoot, "recover-abandoned-lease": token } = values;
	if (siteRoot === undefined) return undefined;
	if (token === undefined) {
		return webRoot === undefined
			? undefined
			: { kind: "publish", webRoot: resolve(webRoot), siteRoot };
	}

	return webRoot === undefined ? { kind: "recover", siteRoot, token } : undefined;
}

function main(): void {
	const command = parseCommand(process.argv.slice(2));
	if (command === undefined) {
		console.error(USAGE);
		process.exit(2);
	}

	try {
		if (command.kind === "publish") publishSite(command.webRoot, command.siteRoot);
		else recoverAbandonedLease(command.siteRoot, command.token);
	} catch (error) {
		log(errorText(error));
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
