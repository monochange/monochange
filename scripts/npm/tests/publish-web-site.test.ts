import assert from "node:assert/strict";
import {
	cpSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	readdirSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { afterEach, describe, test } from "vitest";

import {
	FONT_NAMES,
	listTree,
	promoteGeneration,
	publishSite,
	validateGeneration,
	validateScratch,
} from "../../leptoskit-plugin/publish-web-site.ts";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const fixturePkg = join(repoRoot, "fixtures/tests/website-split-loader-minify/loader-site/pkg");

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

function scratchRoot(tag: string): string {
	return mkdtempSync(join(tmpdir(), `publish-web-${tag}-`));
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
 * and — for the website — the WOFF2 subsets the plugin leaves behind.
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
	writeBytes(join(webRoot, "web/branding/mark.svg"), "mark");
	writeBytes(join(webRoot, "web/branding/social.png"), "png");
	writeBytes(join(webRoot, "web/fonts/hanken-400.ttf"), "source ttf");
	for (const font of FONT_NAMES) {
		writeBytes(join(webRoot, "web/.font-subsets", `${font}.woff2`), `subset ${font}`);
	}

	return { webRoot, siteRoot };
}

function digest(path: string): string {
	return createHash("sha256").update(readFileSync(path)).digest("hex");
}

/** The full published-tree digest, for last-good comparisons. */
function siteDigest(siteRoot: string): string {
	return listTree(siteRoot)
		.map((name) => `${name}:${digest(join(siteRoot, name))}`)
		.join("\n");
}

describe("publish-web-site", () => {
	const roots: string[] = [];

	afterEach(() => {
		for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
	});

	test("publishes a complete tree with both namespaces, one stylesheet, fonts, and twins", () => {
		const { webRoot, siteRoot } = buildScratches("complete");
		roots.push(dirname(webRoot));
		const published = publishSite(webRoot, siteRoot);
		assert.ok(published.files > 0);

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
		// The scratch outputs remain untouched for the SDK's own reporting.
		assert.ok(existsSync(join(webRoot, "web/pkg/monochange_app_bg.wasm")));
	});

	test("a removed public file and stale chunks cannot survive a rebuild", () => {
		const { webRoot, siteRoot } = buildScratches("stale");
		roots.push(dirname(webRoot));
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
		roots.push(dirname(webRoot));
		publishSite(webRoot, siteRoot);
		const before = siteDigest(siteRoot);

		writeFileSync(join(webRoot, "web/pkg/__wasm_split_manifest.json"), "not json");
		assert.throws(() => publishSite(webRoot, siteRoot), /split manifest/);
		assert.equal(siteDigest(siteRoot), before, "a failed publication changed the live site");
		assert.ok(
			!readdirSync(dirname(siteRoot)).some((name) => name.startsWith(".site-staging-")),
			"no staging leftover may remain",
		);
	});

	test("a manifest chunk missing from the scratch aborts publication", () => {
		const { webRoot, siteRoot } = buildScratches("missing-chunk");
		roots.push(dirname(webRoot));
		publishSite(webRoot, siteRoot);
		const before = siteDigest(siteRoot);

		rmSync(join(webRoot, "projects-web/pkg/chunk_11.wasm"));
		assert.throws(() => publishSite(webRoot, siteRoot), /missing chunk/);
		assert.equal(siteDigest(siteRoot), before);
	});

	test("an unreferenced scratch chunk aborts publication", () => {
		const { webRoot, siteRoot } = buildScratches("unreferenced-chunk");
		roots.push(dirname(webRoot));
		writeBytes(join(webRoot, "web/pkg/chunk_42.wasm"), wasmBytes());
		assert.throws(() => publishSite(webRoot, siteRoot), /unreferenced chunk/);
		assert.ok(!existsSync(siteRoot), "nothing may publish from an inconsistent scratch");
	});

	test("diverging stylesheets between the two scratches abort publication", () => {
		const { webRoot, siteRoot } = buildScratches("css-divergence");
		roots.push(dirname(webRoot));
		writeBytes(join(webRoot, "projects-web/pkg/monochange_app.css"), "different bytes");
		assert.throws(() => publishSite(webRoot, siteRoot), /different stylesheets/);
		assert.ok(!existsSync(siteRoot));
	});

	test("an incomplete generation fails validation before anyone can see it", () => {
		const staging = join(scratchRoot("validate"), ".site-staging-x");
		roots.push(dirname(staging));
		mkdirSync(join(staging, "pkg/site"), { recursive: true });
		assert.throws(() => validateGeneration(staging), /missing pkg\/site\/monochange_app\.js/);
	});

	test("a staged generation without compression twins fails validation", () => {
		const { webRoot, siteRoot } = buildScratches("twin-missing");
		roots.push(dirname(webRoot));
		publishSite(webRoot, siteRoot);
		const before = siteDigest(siteRoot);

		// A generation staged by an interrupted compression pass: start from a
		// complete published tree and remove one brotli twin. Validation must
		// reject it before promotion, and the live site must not change.
		const broken = join(scratchRoot("twin-broken"), ".site-staging-broken");
		roots.push(dirname(broken));
		cpSync(siteRoot, broken, { recursive: true });
		rmSync(join(broken, "pkg/site/monochange_app.css.br"));
		assert.throws(
			() => validateGeneration(broken),
			/brotli twin for pkg\/site\/monochange_app\.css/,
		);
		assert.equal(siteDigest(siteRoot), before);
	});

	test("promotion with no staging directory fails and keeps the live site", () => {
		const { webRoot, siteRoot } = buildScratches("no-staging");
		roots.push(dirname(webRoot));
		publishSite(webRoot, siteRoot);
		const before = siteDigest(siteRoot);
		assert.throws(
			() => promoteGeneration(join(webRoot, "no-such-staging"), siteRoot),
			/vanished before promotion/,
		);
		assert.equal(siteDigest(siteRoot), before);
		assert.ok(
			!readdirSync(dirname(siteRoot)).some((name) => name.startsWith(".site-previous-")),
			"no previous-tree leftover may remain",
		);
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
