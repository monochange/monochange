import assert from "node:assert/strict";
import { type ChildProcess, spawn, spawnSync } from "node:child_process";
import {
	chmodSync,
	cpSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	readdirSync,
	rmSync,
	statSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { afterEach, describe, test } from "vitest";

import {
	FONT_NAMES,
	PublishFailure,
	listTree,
	publishSite,
	recoverAbandonedLease,
	validateGeneration,
	validateScratch,
} from "../../leptoskit-plugin/publish-web-site.ts";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const fixturePkg = join(repoRoot, "fixtures/tests/website-split-loader-minify/loader-site/pkg");
const publisherFixtures = join(repoRoot, "fixtures/tests/publish-web-site");
const publisherPath = join(repoRoot, "scripts/leptoskit-plugin/publish-web-site.ts");
const faultyPublisherPath = join(publisherFixtures, "helpers/faulty-publisher.mjs");

const loaderName = "__wasm_split.______________________.js";
/** The chunks the committed fixture manifest references. */
const FIXTURE_CHUNKS = [
	"chunk_11.wasm",
	"chunk_12.wasm",
	"split___book_route_view_17666993872358248534.wasm",
	"split___pricing_route_view_14841084612111332454.wasm",
];
const SITE_ENTRY = "monochange_app";
const PROJECTS_ENTRY = "monochange_projects_web";
/** Bounds every helper process and every wait on one. */
const HELPER_LIMIT_MS = 20_000;
const HELPER_TEST = { timeout: 60_000 };
/** Removal permissions do not bind root, so the denied-removal case needs a regular user. */
const runsAsRoot = process.getuid?.() === 0;

const roots: string[] = [];

function scratchRoot(tag: string): string {
	const root = mkdtempSync(join(tmpdir(), `publish-web-${tag}-`));
	roots.push(root);

	return root;
}

function wasmBytes(): Buffer {
	return Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
}

function writeBytes(path: string, bytes: string | Buffer = "x"): void {
	mkdirSync(dirname(path), { recursive: true });
	writeFileSync(path, bytes);
}

/**
 * Builds a pair of scratch outputs shaped like finished `leptoskit build`
 * runs: complete pkg namespaces (loader, manifest, chunks, entry glue, wasm,
 * shared stylesheet) from the committed fixture, plus public assets, fonts,
 * and — for the website — the WOFF2 subsets the plugin leaves behind. The
 * public `generation.txt` names the generation, so tests can tell published
 * trees apart.
 */
function buildScratches(
	tag: string,
	options?: { skip?: string[] },
): {
	webRoot: string;
	siteRoot: string;
} {
	const webRoot = join(scratchRoot(tag), "run");
	const siteRoot = join(webRoot, "site");
	for (const [outDir, outputName] of [
		["web", SITE_ENTRY],
		["projects-web", PROJECTS_ENTRY],
	] as const) {
		if (options?.skip?.includes(outDir)) continue;
		const pkg = join(webRoot, outDir, "pkg");
		mkdirSync(pkg, { recursive: true });
		for (const name of readdirSync(fixturePkg)) {
			writeBytes(join(pkg, name), readFileSync(join(fixturePkg, name)));
		}
		for (const chunk of FIXTURE_CHUNKS) writeBytes(join(pkg, chunk), wasmBytes());
		writeBytes(join(pkg, `${outputName}.js`), `// ${outputName} entry glue`);
		writeBytes(join(pkg, `${outputName}_bg.wasm`), wasmBytes());
		writeBytes(join(pkg, "monochange_app.css"), "shared css bytes");
	}
	// The website scratch root carries the public mirror and font subsets the
	// real SDK output has — including the TTF font sources under public/fonts.
	writeBytes(join(webRoot, "web/favicon.ico"), "ico");
	writeBytes(join(webRoot, "web/favicon.svg"), "svg");
	writeBytes(join(webRoot, "web/site.webmanifest"), "{}");
	writeBytes(join(webRoot, "web/robots.txt"), "user-agent: *");
	writeBytes(join(webRoot, "web/generation.txt"), tag);
	writeBytes(join(webRoot, "web/branding/mark.svg"), "mark");
	writeBytes(join(webRoot, "web/branding/social.png"), "png");
	writeBytes(join(webRoot, "web/fonts/hanken-400.ttf"), "source ttf");
	for (const font of FONT_NAMES) {
		writeBytes(join(webRoot, "web/.font-subsets", `${font}.woff2`), `subset ${font}`);
	}

	return { webRoot, siteRoot };
}

/** A fresh scratch output whose published tree identifies itself as `tag`. */
function generation(tag: string): string {
	return buildScratches(tag).webRoot;
}

/** A destination in its own artifact directory, apart from every scratch output. */
function destination(tag: string): { site: string; parent: string; lease: string } {
	const parent = join(scratchRoot(tag), "target");
	mkdirSync(parent);

	return { site: join(parent, "site"), parent, lease: join(parent, ".site.publish-lease") };
}

function generationOf(site: string): string {
	return readFileSync(join(site, "generation.txt"), "utf8");
}

function digest(path: string): string {
	return createHash("sha256").update(readFileSync(path)).digest("hex");
}

/** A full tree digest (relative names and contents), for byte-identity comparisons. */
function treeDigest(root: string): string {
	return listTree(root)
		.map((name) => `${name}:${digest(join(root, name))}`)
		.join("\n");
}

function entries(directory: string): string[] {
	return readdirSync(directory).toSorted();
}

/** The owner token a held-lease or recovery-required message tells the operator to use. */
function recoveryToken(message: string): string {
	const match = /--recover-abandoned-lease ([0-9a-f]{16})/u.exec(message);
	assert.ok(match !== null, `the message names no recovery command:\n${message}`);

	return match[1];
}

/** The lease cleanup a helper publisher reported on stdout. */
function cleanupOf(stdout: string): unknown {
	const result: unknown = JSON.parse(stdout);
	assert.ok(typeof result === "object" && result !== null && "cleanup" in result, stdout);

	return result.cleanup;
}

function publishFailure(run: () => unknown): string {
	try {
		run();
	} catch (error) {
		assert.ok(error instanceof PublishFailure, `unexpected error: ${String(error)}`);

		return error.message;
	}

	return assert.fail("the publication unexpectedly succeeded");
}

/** Restores write permission everywhere under `root` so it can be removed. */
function makeWritable(root: string): void {
	if (!existsSync(root) || !statSync(root).isDirectory()) return;
	chmodSync(root, 0o755);
	for (const entry of readdirSync(root, { withFileTypes: true })) {
		if (entry.isDirectory()) makeWritable(join(root, entry.name));
	}
}

type Fault = {
	/** Exact rename source; any source when omitted. */
	from?: string;
	/** Exact rename target. */
	to: string;
	effect: "fail" | "pause";
	/** The pause barrier's path prefix (`.paused` / `.resume`). */
	barrier?: string;
};

type HelperExit = {
	code: number | null;
	signal: NodeJS.Signals | null;
	stdout: string;
	stderr: string;
};

type Helper = { child: ChildProcess; exit: Promise<HelperExit> };

const helpers: Helper[] = [];

/** Runs the real publisher in an owned child process with the given rename faults. */
function startPublisher(webRoot: string, site: string, faults: Fault[]): Helper {
	const plan = { run: { publish: { webRoot, siteRoot: site } }, faults };
	const child = spawn(
		process.execPath,
		[faultyPublisherPath, publisherPath, JSON.stringify(plan)],
		{
			stdio: ["ignore", "pipe", "pipe"],
		},
	);
	let stdout = "";
	let stderr = "";
	child.stdout?.on("data", (chunk: Buffer) => {
		stdout += chunk.toString();
	});
	child.stderr?.on("data", (chunk: Buffer) => {
		stderr += chunk.toString();
	});
	const exit = new Promise<HelperExit>((resolveExit, reject) => {
		const limit = setTimeout(() => child.kill("SIGKILL"), HELPER_LIMIT_MS);
		child.once("error", (error) => {
			clearTimeout(limit);
			reject(error);
		});
		child.once("close", (code, signal) => {
			clearTimeout(limit);
			resolveExit({ code, signal, stdout, stderr });
		});
	});
	const helper = { child, exit };
	helpers.push(helper);

	return helper;
}

async function waitUntilPaused(helper: Helper, barrier: string): Promise<void> {
	const deadline = Date.now() + HELPER_LIMIT_MS;
	while (!existsSync(`${barrier}.paused`)) {
		if (helper.child.exitCode !== null || helper.child.signalCode !== null) {
			const exit = await helper.exit;
			assert.fail(`the helper exited before pausing at ${barrier}: ${JSON.stringify(exit)}`);
		}
		if (Date.now() > deadline) assert.fail(`the helper never paused at ${barrier}`);
		await delay(10);
	}
}

function resume(barrier: string): void {
	writeFileSync(`${barrier}.resume`, "go\n");
}

/** A barrier path outside every destination, so it never shows up beside a site. */
function barrierPath(name: string): string {
	return join(scratchRoot(`barrier-${name}`), name);
}

function runCli(args: string[]): { status: number | null; stderr: string } {
	const result = spawnSync(process.execPath, [publisherPath, ...args], {
		encoding: "utf8",
		timeout: HELPER_LIMIT_MS,
	});

	return { status: result.status, stderr: result.stderr };
}

afterEach(async () => {
	for (const { child } of helpers) {
		if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
	}
	await Promise.allSettled(helpers.splice(0).map((helper) => helper.exit));
	for (const root of roots.splice(0)) {
		makeWritable(root);
		rmSync(root, { recursive: true, force: true });
	}
});

describe("publish-web-site", () => {
	test("publishes a complete tree with both namespaces, one stylesheet, fonts, and twins", () => {
		const { webRoot, siteRoot } = buildScratches("complete");
		const published = publishSite(webRoot, siteRoot);
		assert.ok(published.files > 0);
		assert.deepEqual(published.cleanup, { kind: "released" });

		// Both frontends complete in their own namespaces.
		for (const [namespace, outputName] of [
			["site", SITE_ENTRY],
			["projects", PROJECTS_ENTRY],
		] as const) {
			for (const name of [
				`${outputName}.js`,
				`${outputName}_bg.wasm`,
				loaderName,
				"__wasm_split_manifest.json",
				...FIXTURE_CHUNKS,
			]) {
				assert.ok(
					existsSync(join(siteRoot, "pkg", namespace, name)),
					`pkg/${namespace}/${name} must be published`,
				);
			}
		}
		// Exactly one stylesheet, at the canonical shared location.
		assert.ok(existsSync(join(siteRoot, "pkg/site/monochange_app.css")));
		assert.ok(!existsSync(join(siteRoot, "pkg/projects/monochange_app.css")));
		// The website build's public mirror sits at the root.
		assert.ok(existsSync(join(siteRoot, "robots.txt")));
		assert.ok(existsSync(join(siteRoot, "branding/social.png")));
		// The WOFF2 subsets became the deployed fonts; the TTF sources the SDK
		// mirrors from public/fonts are replaced, not shipped alongside.
		for (const font of FONT_NAMES) {
			assert.equal(
				readFileSync(join(siteRoot, "fonts", `${font}.woff2`), "utf8"),
				`subset ${font}`,
			);
			assert.ok(!existsSync(join(siteRoot, "fonts", `${font}.ttf`)), `${font}.ttf must not ship`);
		}
		// Every compressible asset carries nonempty gzip and brotli twins.
		for (const name of listTree(siteRoot)) {
			if (!/\.(?:wasm|js|css|json|svg|webmanifest|txt)$/u.test(name)) continue;
			assert.ok(existsSync(join(siteRoot, `${name}.gz`)), `${name}.gz`);
			assert.ok(existsSync(join(siteRoot, `${name}.br`)), `${name}.br`);
		}
		// No publisher state remains beside the site, and nothing lands inside it.
		assert.deepEqual(entries(webRoot), ["projects-web", "site", "web"]);
		assert.ok(!listTree(siteRoot).some((name) => /owner-|publish-/u.test(name)));
		// The scratch outputs remain untouched for the SDK's own reporting.
		assert.ok(existsSync(join(webRoot, "web/pkg/monochange_app_bg.wasm")));
	});

	test("a removed public file and stale chunks cannot survive a rebuild", () => {
		const { webRoot, siteRoot } = buildScratches("stale");
		publishSite(webRoot, siteRoot);

		// First generation carries a public file and an extra chunk.
		writeBytes(join(siteRoot, "icons/retired.svg"), "old");
		writeBytes(join(siteRoot, "pkg/site/chunk_99.wasm"), "stale chunk");

		// Rebuild: the mirror no longer carries the retired asset, and the
		// scratch has no chunk_99. The published tree must match exactly.
		rmSync(join(webRoot, "web/robots.txt"));
		publishSite(webRoot, siteRoot);
		assert.ok(!existsSync(join(siteRoot, "icons/retired.svg")), "stale public file survived");
		assert.ok(!existsSync(join(siteRoot, "pkg/site/chunk_99.wasm")), "stale chunk survived");
		assert.ok(!existsSync(join(siteRoot, "robots.txt")), "removed public file survived");
		assert.ok(existsSync(join(siteRoot, "favicon.svg")));
	});

	test("a corrupt scratch manifest aborts publication and keeps the last good site", () => {
		const { webRoot, siteRoot } = buildScratches("bad-manifest");
		publishSite(webRoot, siteRoot);
		const before = treeDigest(siteRoot);
		const neighbours = entries(webRoot);

		writeFileSync(join(webRoot, "web/pkg/__wasm_split_manifest.json"), "not json");
		assert.throws(() => publishSite(webRoot, siteRoot), /split manifest/);
		assert.equal(treeDigest(siteRoot), before, "a failed publication changed the live site");
		assert.deepEqual(entries(webRoot), neighbours, "no publisher state may remain");
	});

	test("a manifest chunk missing from the scratch aborts publication", () => {
		const { webRoot, siteRoot } = buildScratches("missing-chunk");
		publishSite(webRoot, siteRoot);
		const before = treeDigest(siteRoot);

		rmSync(join(webRoot, "projects-web/pkg/chunk_11.wasm"));
		assert.throws(() => publishSite(webRoot, siteRoot), /missing chunk/);
		assert.equal(treeDigest(siteRoot), before);
	});

	test("an unreferenced scratch chunk aborts publication", () => {
		const { webRoot, siteRoot } = buildScratches("unreferenced-chunk");
		writeBytes(join(webRoot, "web/pkg/chunk_42.wasm"), wasmBytes());
		assert.throws(() => publishSite(webRoot, siteRoot), /unreferenced chunk/);
		assert.ok(!existsSync(siteRoot), "nothing may publish from an inconsistent scratch");
	});

	test("diverging stylesheets between the two scratches abort publication", () => {
		const { webRoot, siteRoot } = buildScratches("css-divergence");
		writeBytes(join(webRoot, "projects-web/pkg/monochange_app.css"), "different bytes");
		assert.throws(() => publishSite(webRoot, siteRoot), /different stylesheets/);
		assert.ok(!existsSync(siteRoot));
	});

	test("an incomplete generation fails validation before anyone can see it", () => {
		const staging = join(scratchRoot("validate"), "staging");
		mkdirSync(join(staging, "pkg/site"), { recursive: true });
		assert.throws(() => validateGeneration(staging), /missing pkg\/site\/monochange_app\.js/);
	});

	test("a staged generation without compression twins fails validation", () => {
		const { webRoot, siteRoot } = buildScratches("twin-missing");
		publishSite(webRoot, siteRoot);
		const before = treeDigest(siteRoot);

		// A generation staged by an interrupted compression pass: start from a
		// complete published tree and remove one brotli twin. Validation must
		// reject it before promotion, and the live site must not change.
		const broken = join(scratchRoot("twin-broken"), "staging");
		cpSync(siteRoot, broken, { recursive: true });
		rmSync(join(broken, "pkg/site/monochange_app.css.br"));
		assert.throws(
			() => validateGeneration(broken),
			/brotli twin for pkg\/site\/monochange_app\.css/,
		);
		assert.equal(treeDigest(siteRoot), before);
	});

	test("validateScratch rejects an absent or partial scratch build", () => {
		const { webRoot } = buildScratches("partial", { skip: ["projects-web"] });
		assert.throws(
			() =>
				validateScratch(webRoot, {
					outDir: "projects-web",
					namespace: "projects",
					outputName: PROJECTS_ENTRY,
				}),
			/scratch build/,
		);
		const { webRoot: full } = buildScratches("full");
		rmSync(join(full, "web/pkg/monochange_app_bg.wasm"));
		assert.throws(
			() => validateScratch(full, { outDir: "web", namespace: "site", outputName: SITE_ENTRY }),
			/monochange_app_bg\.wasm/,
		);
	});
});

describe("publish-web-site exclusive ownership", () => {
	test("publication never touches older-format leftovers, other leases, retained trees, or databases", () => {
		const { site, parent } = destination("neighbours");
		const neighbours = join(publisherFixtures, "neighbours");
		cpSync(neighbours, parent, { recursive: true });
		const before = treeDigest(parent);

		publishSite(generation("first"), site);
		const second = publishSite(generation("second"), site);

		assert.deepEqual(second.cleanup, { kind: "released" });
		assert.equal(generationOf(site), "second");
		assert.deepEqual(entries(parent), [...entries(neighbours), "site"].toSorted());
		const after = listTree(parent)
			.filter((name) => !name.startsWith("site/"))
			.map((name) => `${name}:${digest(join(parent, name))}`)
			.join("\n");
		assert.equal(after, before, "a neighbour of the site root changed");
	});

	test(
		"a second publisher can neither take nor lose the owner's superseded tree during retirement",
		HELPER_TEST,
		async () => {
			const { site, parent, lease } = destination("interleave");
			publishSite(generation("zero"), site);
			const zero = treeDigest(site);

			// A commits its generation and stops before releasing the lease —
			// the point where the rejected design let B move A's commit away.
			const committed = barrierPath("a-committed");
			const a = startPublisher(generation("a"), site, [
				{ from: join(lease, "staging"), to: site, effect: "pause", barrier: committed },
			]);
			await waitUntilPaused(a, committed);
			assert.equal(generationOf(site), "a");
			assert.equal(treeDigest(join(lease, "previous")), zero);
			const leaseWhileOwned = treeDigest(lease);
			const siteWhileOwned = treeDigest(site);
			const parentWhileOwned = entries(parent);

			// B fails before changing anything: A's record, superseded tree, and
			// committed site are byte-identical, and B left no claim behind.
			const refused = publishFailure(() => publishSite(generation("b"), site));
			assert.match(refused, /is owned by another publication/u);
			assert.match(refused, /changed nothing/u);
			assert.equal(treeDigest(lease), leaseWhileOwned);
			assert.equal(treeDigest(site), siteWhileOwned);
			assert.deepEqual(entries(parent), parentWhileOwned);

			// A finishes: its generation is published and its retirement complete.
			resume(committed);
			const finished = await a.exit;
			assert.equal(finished.code, 0, finished.stderr);
			assert.deepEqual(cleanupOf(finished.stdout), { kind: "released" });
			assert.equal(treeDigest(site), siteWhileOwned);
			assert.deepEqual(entries(parent), ["site"]);

			// B retries and its promotion fails: the last-good A is restored.
			const b = startPublisher(generation("b"), site, [
				{ from: join(lease, "staging"), to: site, effect: "fail" },
			]);
			const failed = await b.exit;
			assert.equal(failed.code, 1);
			assert.match(failed.stderr, /injected i\/o error/u);
			assert.match(failed.stderr, /the last-good tree was restored/u);
			assert.equal(generationOf(site), "a");
			assert.equal(treeDigest(site), siteWhileOwned);
			assert.deepEqual(entries(parent), ["site"]);
		},
	);

	test(
		"an interrupted publication keeps the last-good tree under its lease until explicit recovery",
		HELPER_TEST,
		async () => {
			const { site, parent, lease } = destination("interrupted");
			publishSite(generation("zero"), site);
			const zero = treeDigest(site);

			// Kill the publisher between the two renames: the site root is absent.
			const swapped = barrierPath("swapped");
			const interrupted = startPublisher(generation("lost"), site, [
				{ from: site, to: join(lease, "previous"), effect: "pause", barrier: swapped },
			]);
			await waitUntilPaused(interrupted, swapped);
			interrupted.child.kill("SIGKILL");
			assert.equal((await interrupted.exit).signal, "SIGKILL");
			assert.ok(!existsSync(site));
			assert.equal(treeDigest(join(lease, "previous")), zero);
			assert.ok(existsSync(join(lease, "staging")));
			const abandoned = treeDigest(lease);

			// Every later publication fails closed and names what the lease keeps.
			const next = generation("next");
			const refused = publishFailure(() => publishSite(next, site));
			assert.match(refused, /changed nothing/u);
			assert.ok(refused.includes(`the last-good tree ${join(lease, "previous")}`), refused);
			assert.ok(refused.includes(`an uncommitted generation ${join(lease, "staging")}`), refused);
			const token = recoveryToken(refused);
			const cli = runCli(["--web-root", next, "--site-root", site]);
			assert.equal(cli.status, 1);
			assert.ok(cli.stderr.includes(`--recover-abandoned-lease ${token}`), cli.stderr);

			// Recovery refuses any owner but the abandoned one.
			assert.throws(
				() => recoverAbandonedLease(site, "0".repeat(16)),
				/is not held by owner 0000000000000000/u,
			);
			assert.equal(treeDigest(lease), abandoned);
			assert.ok(!existsSync(site));

			// The operator's explicit recovery restores the last-good tree.
			const recovered = runCli(["--site-root", site, "--recover-abandoned-lease", token]);
			assert.equal(recovered.status, 0, recovered.stderr);
			assert.match(recovered.stderr, /restored the last-good tree/u);
			assert.equal(treeDigest(site), zero);
			assert.deepEqual(entries(parent), ["site"]);

			publishSite(next, site);
			assert.equal(generationOf(site), "next");
		},
	);

	test(
		"a failed promotion whose rollback also fails keeps the last-good tree in the held lease",
		HELPER_TEST,
		async () => {
			const { site, parent, lease } = destination("rollback");
			publishSite(generation("zero"), site);
			const zero = treeDigest(site);
			const previous = join(lease, "previous");

			const run = startPublisher(generation("doomed"), site, [
				{ from: join(lease, "staging"), to: site, effect: "fail" },
				{ from: previous, to: site, effect: "fail" },
			]);
			const failed = await run.exit;
			assert.equal(failed.code, 1);
			assert.ok(
				failed.stderr.includes(`The last-good tree is retained at ${previous}`),
				failed.stderr,
			);
			assert.ok(!existsSync(site));
			assert.equal(treeDigest(previous), zero);

			// The retained tree stays out of reach until the explicit recovery.
			const token = recoveryToken(failed.stderr);
			assert.match(
				publishFailure(() => publishSite(generation("next"), site)),
				/changed nothing/u,
			);
			assert.equal(treeDigest(previous), zero);

			const recovered = recoverAbandonedLease(site, token);
			assert.deepEqual(recovered.tree, { kind: "restored" });
			assert.deepEqual(recovered.cleanup, { kind: "released" });
			assert.equal(treeDigest(site), zero);
			assert.deepEqual(entries(parent), ["site"]);
		},
	);

	test(
		"a live owner holds its destination while a sibling destination publishes independently",
		HELPER_TEST,
		async () => {
			const { site, parent, lease } = destination("siblings");
			const docs = join(parent, "docs");
			publishSite(generation("docs-zero"), docs);

			// A owns `site` and stops right after taking the lease.
			const leased = barrierPath("leased");
			const a = startPublisher(generation("a"), site, [
				{ to: lease, effect: "pause", barrier: leased },
			]);
			await waitUntilPaused(a, leased);

			// The sibling destination is unaffected; the owned one is refused.
			publishSite(generation("docs-one"), docs);
			assert.equal(generationOf(docs), "docs-one");
			const docsTree = treeDigest(docs);
			assert.match(
				publishFailure(() => publishSite(generation("c"), site)),
				/is held by owner/u,
			);

			resume(leased);
			const finished = await a.exit;
			assert.equal(finished.code, 0, finished.stderr);
			assert.equal(generationOf(site), "a");
			assert.equal(treeDigest(docs), docsTree);
			assert.deepEqual(entries(parent), ["docs", "site"]);
		},
	);

	test("an empty unknown lease is preserved instead of replaced", () => {
		const { site, parent, lease } = destination("empty-lease");
		publishSite(generation("zero"), site);
		const zero = treeDigest(site);
		mkdirSync(lease);

		const refused = publishFailure(() => publishSite(generation("next"), site));
		assert.match(refused, /its owner cannot be identified; this publication changed nothing/u);
		assert.deepEqual(entries(lease), []);
		assert.equal(treeDigest(site), zero);
		assert.deepEqual(entries(parent), [".site.publish-lease", "site"]);
	});

	test("a lease without an owner record fails closed and cannot be adopted", () => {
		const { site, parent, lease } = destination("ownerless");
		publishSite(generation("zero"), site);
		const zero = treeDigest(site);
		assert.throws(
			() => recoverAbandonedLease(site, "0123456789abcdef"),
			/no publication lease at .* nothing was changed/u,
		);

		cpSync(join(publisherFixtures, "ownerless-lease"), lease, { recursive: true });
		const foreign = treeDigest(lease);

		const refused = publishFailure(() => publishSite(generation("next"), site));
		assert.match(refused, /its owner cannot be identified; this publication changed nothing/u);
		assert.throws(() => recoverAbandonedLease(site, "0123456789abcdef"), /is not held by owner/u);
		assert.throws(() => recoverAbandonedLease(site, "../../escape"), /invalid owner token/u);

		assert.equal(treeDigest(lease), foreign);
		assert.equal(treeDigest(site), zero);
		assert.deepEqual(entries(parent), [".site.publish-lease", "site"]);
	});

	test.skipIf(runsAsRoot)(
		"a retirement that cannot remove the superseded tree is a retained leftover, not a failed publication",
		() => {
			const { site, parent, lease } = destination("retire");
			publishSite(generation("zero"), site);
			// A real permission error: the superseded tree's `branding` entries
			// cannot be unlinked, so removing the retired lease fails.
			chmodSync(join(site, "branding"), 0o555);

			const published = publishSite(generation("one"), site);
			assert.equal(generationOf(site), "one");
			assert.ok(!existsSync(lease), "the destination must be free for the next publication");
			const { cleanup } = published;
			if (cleanup.kind !== "retained") assert.fail(`expected a retained leftover: ${cleanup.kind}`);
			assert.equal(dirname(cleanup.path), parent);
			assert.match(cleanup.path, /\/\.site\.publish-retired-[0-9a-f]{16}$/u);
			assert.notEqual(cleanup.reason, "");
			assert.ok(existsSync(join(cleanup.path, "previous/branding/mark.svg")));
			const retained = treeDigest(cleanup.path);

			// Later publications never remove or change the retained leftover.
			const next = publishSite(generation("two"), site);
			assert.deepEqual(next.cleanup, { kind: "released" });
			assert.equal(generationOf(site), "two");
			assert.equal(treeDigest(cleanup.path), retained);
		},
	);

	test(
		"recovery after an interrupted retirement sets the superseded tree aside instead of deleting it",
		HELPER_TEST,
		async () => {
			const { site, parent, lease } = destination("set-aside");
			publishSite(generation("zero"), site);
			const zero = treeDigest(site);

			const committed = barrierPath("committed");
			const interrupted = startPublisher(generation("one"), site, [
				{ from: join(lease, "staging"), to: site, effect: "pause", barrier: committed },
			]);
			await waitUntilPaused(interrupted, committed);
			interrupted.child.kill("SIGKILL");
			await interrupted.exit;
			assert.equal(generationOf(site), "one");

			const refused = publishFailure(() => publishSite(generation("two"), site));
			assert.ok(
				refused.includes(
					`the tree ${join(lease, "previous")}, retained while the site root also exists`,
				),
				refused,
			);

			const recovered = recoverAbandonedLease(site, recoveryToken(refused));
			const { tree } = recovered;
			if (tree.kind !== "set-aside") assert.fail(`expected the tree set aside: ${tree.kind}`);
			assert.match(tree.path, /\/\.site\.publish-retained-[0-9a-f]{16}$/u);
			assert.equal(treeDigest(tree.path), zero);
			assert.equal(generationOf(site), "one");
			assert.deepEqual(entries(parent), [basename(tree.path), "site"]);

			publishSite(generation("two"), site);
			assert.equal(generationOf(site), "two");
			assert.equal(treeDigest(tree.path), zero);
		},
	);

	test(
		"an owner whose lease was adopted leaves it untouched instead of releasing it",
		HELPER_TEST,
		async () => {
			const { site, lease } = destination("lost");
			publishSite(generation("zero"), site);
			const zero = treeDigest(site);

			const committed = barrierPath("committed");
			const owner = startPublisher(generation("one"), site, [
				{ from: join(lease, "staging"), to: site, effect: "pause", barrier: committed },
			]);
			await waitUntilPaused(owner, committed);

			// An operator wrongly recovers the live owner's lease.
			const token = recoveryToken(publishFailure(() => publishSite(generation("two"), site)));
			const recovered = recoverAbandonedLease(site, token);
			if (recovered.tree.kind !== "set-aside")
				assert.fail("expected the superseded tree set aside");
			const setAside = recovered.tree.path;

			// The owner can no longer prove ownership, so it releases nothing.
			resume(committed);
			const finished = await owner.exit;
			assert.equal(finished.code, 0, finished.stderr);
			assert.deepEqual(cleanupOf(finished.stdout), { kind: "lost", path: lease });
			assert.match(finished.stderr, /no longer carries this publication's owner record/u);
			assert.equal(generationOf(site), "one");
			assert.equal(treeDigest(setAside), zero);
		},
	);

	test("the CLI separates usage errors (2) from publication failures (1)", () => {
		const { site } = destination("usage");
		for (const args of [
			[],
			["--site-root", site],
			["--web-root", "web", "--site-root", site, "--recover-abandoned-lease", "0".repeat(16)],
			["--web-root", "web", "--site-root", site, "extra"],
		]) {
			assert.equal(runCli(args).status, 2, args.join(" "));
		}
		assert.equal(
			runCli(["--site-root", site, "--recover-abandoned-lease", "0".repeat(16)]).status,
			1,
		);
	});
});
